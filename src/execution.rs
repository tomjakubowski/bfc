#[cfg(test)]
use bfir::parse;

use bfir::Instruction;
use bfir::Instruction::*;

use bounds::highest_cell_index;

#[derive(Debug,Clone,PartialEq,Eq)]
pub struct ExecutionState {
    pub instr_ptr: usize,
    pub cells: Vec<u8>,
    pub cell_ptr: isize,
    pub outputs: Vec<u8>,
}

#[derive(Debug,PartialEq,Eq)]
enum Outcome {
    // Return the number of steps remaining at completion.
    Completed(u64),
    ReachedRuntimeValue,
    RuntimeError,
    OutOfSteps,
}

// It takes around 1 million steps to finish executing bottles.bf at
// compile time. This is intolerably slow for debug builds of bfc, but
// instant on a release build.
pub const MAX_STEPS: u64 = 10000000;

/// Compile time speculative execution of instructions. We return the
/// final state of the cells, any print side effects, and the point in
/// the code we reached.
pub fn execute(instrs: &Vec<Instruction>, steps: u64) -> ExecutionState {
    let cells = vec![0; (highest_cell_index(instrs) + 1) as usize];
    let state = ExecutionState {
        instr_ptr: 0, cells: cells, cell_ptr: 0, outputs: vec![] };
    let (final_state, _) = execute_inner(instrs, state, steps);
    final_state
}

fn execute_inner(instrs: &Vec<Instruction>, state: ExecutionState, steps: u64)
                 -> (ExecutionState, Outcome) {
    let mut steps_left = steps;
    let mut state = state;

    while state.instr_ptr < instrs.len() && steps_left > 0 {
        let cell_ptr = state.cell_ptr as usize;
        match &instrs[state.instr_ptr] {
            &Increment(amount) => {
                state.cells[cell_ptr] = state.cells[cell_ptr].wrapping_add(amount);
                state.instr_ptr += 1;
            }
            &Set(amount) => {
                state.cells[cell_ptr] = amount;
                state.instr_ptr += 1;
            }
            &PointerIncrement(amount) => {
                // TODO: PointerIncrement should use an isize.
                let new_cell_ptr = state.cell_ptr + amount as isize;
                if new_cell_ptr < 0 || new_cell_ptr >= state.cells.len() as isize {
                    return (state, Outcome::RuntimeError);
                } else {
                    state.cell_ptr = new_cell_ptr;
                    state.instr_ptr += 1;
                }
            }
            &Write => {
                let cell_value = state.cells[state.cell_ptr as usize];
                state.outputs.push(cell_value);
                state.instr_ptr += 1;
            }
            &Read => {
                return (state, Outcome::ReachedRuntimeValue);
            }
            &Loop(ref body) => {
                if state.cells[state.cell_ptr as usize] == 0 {
                    // Step over the loop because the current cell is
                    // zero.
                    state.instr_ptr += 1;
                } else {
                    // Execute the loop body.
                    let loop_body_state = ExecutionState { instr_ptr: 0, .. state.clone() };
                    let (state_after, loop_outcome) = execute_inner(body, loop_body_state, steps_left);
                    if let &Outcome::Completed(remaining_steps) = &loop_outcome {
                        // We finished executing a loop iteration, so store its side effects.
                        state.cells = state_after.cells;
                        state.outputs = state_after.outputs;
                        state.cell_ptr = state_after.cell_ptr;
                        // We've run several steps during the loop body, so update for that too.
                        steps_left = remaining_steps;
                    }
                    else {
                        // We couldn't evaluate the loop body.
                        return (state, loop_outcome);
                    }
                }
            }
        }

        steps_left -= 1;
    }

    if steps_left == 0 {
        (state, Outcome::OutOfSteps)
    } else {
        (state, Outcome::Completed(steps_left))
    }
}

/// We can't evaluate outputs of runtime values at compile time.
#[test]
fn cant_evaluate_inputs() {
    let instrs = parse(",.").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 0, cells: vec![0], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn increment_executed() {
    let instrs = parse("+").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 1, cells: vec![1], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn set_executed() {
    let instrs = vec![Set(2)];
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 1, cells: vec![2], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn set_wraps() {
    let instrs = vec![Set(255)];
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 1, cells: vec![255], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn decrement_executed() {
    let instrs = parse("-").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 1, cells: vec![255], cell_ptr: 0, outputs: vec![],
        });
}

// TODO: find out what the most common BF implementation choice is here.
#[test]
fn increment_wraps() {
    let instrs = vec![Increment(255), Increment(1)];
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 2, cells: vec![0], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn ptr_increment_executed() {
    let instrs = parse(">").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 1, cells: vec![0, 0], cell_ptr: 1, outputs: vec![],
        });
}

// TODO: it would be nice to emit a warning in this case, as it's
// clearly a user error.
#[test]
fn ptr_out_of_range() {
    let instrs = parse("<").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 0, cells: vec![0], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn limit_to_steps_specified() {
    let instrs = parse("++++").unwrap();
    let final_state = execute(&instrs, 2);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 2, cells: vec![2], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn write_executed() {
    let instrs = parse("+.").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 2, cells: vec![1], cell_ptr: 0, outputs: vec![1],
        });
}

#[test]
fn loop_executed() {
    let instrs = parse("++[-]").unwrap();
    let final_state = execute(&instrs, MAX_STEPS);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 3, cells: vec![0], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn loop_up_to_step_limit() {
    let instrs = parse("++[-]").unwrap();
    // Assuming we take one step to enter the loop, we will execute
    // the loop body once.
    let final_state = execute(&instrs, 4);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 2, cells: vec![1], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn loop_with_read_body() {
    // We should return the state before the loop is executed, since
    // we can't execute the whole loop.
    let instrs = parse("+[+,]").unwrap();
    let final_state = execute(&instrs, 4);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 1, cells: vec![1], cell_ptr: 0, outputs: vec![],
        });
}

#[test]
fn up_to_infinite_loop_executed() {
    let instrs = parse("++[]").unwrap();
    let final_state = execute(&instrs, 20);

    assert_eq!(
        final_state, ExecutionState {
            instr_ptr: 2, cells: vec![2], cell_ptr: 0, outputs: vec![],
        });
}

#[quickcheck]
fn instr_ptr_in_bounds(instrs: Vec<Instruction>) -> bool {
    let state = execute(&instrs, 100);
    state.instr_ptr <= instrs.len()
}

#[quickcheck]
fn cell_ptr_in_bounds(instrs: Vec<Instruction>) -> bool {
    let state = execute(&instrs, 100);
    (state.cell_ptr >= 0) &&
        (state.cell_ptr <= state.cells.len() as isize)
}

#[test]
fn arithmetic_error_nested_loops() {
    // Regression test, based on a snippet from
    // mandlebrot.bf. Previously, if the first element in a loop was
    // another loop, we had arithmetic overflow.
    let instrs = parse("+[[>>>>>>>>>]+>>>>>>>>>-]").unwrap();
    execute(&instrs, MAX_STEPS);
}
