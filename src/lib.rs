#[macro_use]
extern crate lalrpop_util;

#[macro_use]
extern crate anyhow;

lalrpop_mod!(pub grammar);

mod ast;
mod ty;
