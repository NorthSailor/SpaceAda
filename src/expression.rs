
use std::boxed::Box;
use ada_grammar;

#[derive(PartialEq, Clone, Debug)]
pub enum Expression {
    Binary(String, Box<Expression>, Box<Expression>),
    Variable(String),
    IntValue(i32),
    FunctionCall(String, Vec<Expression>),
}

#[test]
fn test_identifiers() {
    assert_eq!(ada_grammar::identifier("test_x"), Ok("test_x".to_string()));
    assert!(ada_grammar::identifier("3numberfirst").is_err());
    assert!(ada_grammar::identifier("a-n").is_err());
}

#[test]
fn test_int_values() {
    assert_eq!(ada_grammar::int_value("32"), Ok(32));
    assert_eq!(ada_grammar::int_value("-42"), Ok(-42));
    assert!(ada_grammar::int_value("ab4").is_err());
    assert!(ada_grammar::int_value("-3e").is_err());
}

#[test]
fn test_terminal_expressions() {
    assert_eq!(ada_grammar::expression("11"), Ok(Expression::IntValue(11)));
    assert_eq!(ada_grammar::expression("Pitch"),
               Ok(Expression::Variable("Pitch".to_string())));
}

#[test]
fn test_arithmetic() {}