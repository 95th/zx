#[macro_use]
extern crate lalrpop_util;

#[macro_use]
extern crate anyhow;

lalrpop_mod!(grammar);

mod ast;
mod reachability;
mod ty;

pub fn run(source: &str) {
    let parser = grammar::ScriptParser::new();
    let script = parser.parse(source).unwrap();

    let mut typeck = ty::TypeckState::new();
    typeck.check_script(&script).unwrap();
}
