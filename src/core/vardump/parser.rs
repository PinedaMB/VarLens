use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "core/vardump/grammar.pest"]
pub struct VarDumpParser;
