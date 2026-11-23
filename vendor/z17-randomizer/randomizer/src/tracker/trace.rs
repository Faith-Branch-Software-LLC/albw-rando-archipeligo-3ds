//! Records a logic rule as a decision diagram. The rules are Rust functions, so the only way to
//! learn one is to run it: every question it asks about the player's items goes to a [`Tracer`],
//! which answers from a fixed list of answers and notes new questions. Running the rule again with
//! each answer to the first new question walks every path the rule can take.

use crate::filler::filler_item::Randomizable;
use std::collections::HashMap;
use std::sync::Mutex;

/// A question a logic rule asks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Var {
    /// The player has this item.
    Has(Randomizable),
    /// The player has at least this many of these items.
    AtLeast(Vec<Randomizable>, u8),
    /// The player has at least this many quarters of a heart.
    Hearts(u16),
    /// The player can get this many rupees.
    Rupees(u16),
}

#[derive(Debug, Default)]
pub struct Tracer {
    state: Mutex<Run>,
}

#[derive(Debug, Default)]
struct Run {
    /// The answers to give, in the order the questions come.
    answers: Vec<bool>,
    /// The questions asked in this run, in order, without repeats.
    asked: Vec<Var>,
}

impl Tracer {
    pub fn ask(&self, var: Var) -> bool {
        let mut run = self.state.lock().unwrap();
        let index = match run.asked.iter().position(|asked| *asked == var) {
            Some(index) => index,
            None => {
                run.asked.push(var);
                run.asked.len() - 1
            }
        };
        run.answers.get(index).copied().unwrap_or(false)
    }
}

/// A reference to a decision diagram node, or one of the two results.
pub type NodeRef = u32;
pub const TRUE: NodeRef = u32::MAX;
pub const FALSE: NodeRef = u32::MAX - 1;

/// Decision diagram nodes shared by every rule of a seed.
#[derive(Default)]
pub struct Diagram {
    pub vars: Vec<Var>,
    var_ids: HashMap<Var, u32>,
    /// (question, node if no, node if yes)
    pub nodes: Vec<(u32, NodeRef, NodeRef)>,
    node_ids: HashMap<(u32, NodeRef, NodeRef), NodeRef>,
    /// Runs left before a rule is too big to record.
    budget: usize,
}

/// A rule bigger than this many runs is recorded as always passing, so the map never hides a
/// check the player can reach.
const RUN_LIMIT: usize = 4096;

impl Diagram {
    /// Records `rule`, which reads the player's items through `tracer`.
    pub fn record(&mut self, tracer: &Tracer, rule: &dyn Fn() -> bool) -> NodeRef {
        self.budget = RUN_LIMIT;
        self.explore(tracer, rule, &mut Vec::new()).unwrap_or(TRUE)
    }

    fn explore(&mut self, tracer: &Tracer, rule: &dyn Fn() -> bool, answers: &mut Vec<bool>) -> Option<NodeRef> {
        self.budget = self.budget.checked_sub(1)?;
        {
            let mut run = tracer.state.lock().unwrap();
            run.answers.clone_from(answers);
            run.asked.clear();
        }
        let result = rule();
        let next = {
            let run = tracer.state.lock().unwrap();
            run.asked.get(answers.len()).cloned()
        };
        let Some(var) = next else {
            return Some(if result { TRUE } else { FALSE });
        };
        answers.push(false);
        let no = self.explore(tracer, rule, answers);
        answers.pop();
        let no = no?;
        answers.push(true);
        let yes = self.explore(tracer, rule, answers);
        answers.pop();
        Some(self.node(var, no, yes?))
    }

    fn node(&mut self, var: Var, no: NodeRef, yes: NodeRef) -> NodeRef {
        if no == yes {
            return no;
        }
        let var = match self.var_ids.get(&var) {
            Some(&id) => id,
            None => {
                let id = self.vars.len() as u32;
                self.vars.push(var.clone());
                self.var_ids.insert(var, id);
                id
            }
        };
        let key = (var, no, yes);
        if let Some(&id) = self.node_ids.get(&key) {
            return id;
        }
        let id = self.nodes.len() as NodeRef;
        self.nodes.push(key);
        self.node_ids.insert(key, id);
        id
    }

    /// Evaluates a recorded rule with answers from `answer`, for tests.
    pub fn eval(&self, mut node: NodeRef, answer: &dyn Fn(&Var) -> bool) -> bool {
        loop {
            match node {
                TRUE => return true,
                FALSE => return false,
                _ => {
                    let (var, no, yes) = self.nodes[node as usize];
                    node = if answer(&self.vars[var as usize]) { yes } else { no };
                }
            }
        }
    }
}
