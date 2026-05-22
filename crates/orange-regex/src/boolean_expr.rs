//! Boolean expression parser for AND/OR/NOT regex combinations.
//!
//! Grammar (recursive descent):
//!
//!   expr     → and_expr (OR and_expr)*
//!   and_expr → not_expr (AND not_expr)*
//!   not_expr → NOT not_expr | atom
//!   atom     → '(' expr ')' | PATTERN
//!
//! Operator precedence (highest to lowest):
//!   NOT > AND > OR

use crate::RegexError;

/// A parsed boolean expression tree.
#[derive(Debug, Clone, PartialEq)]
pub enum BooleanExpr {
    /// A leaf pattern to match.
    Pattern(String),
    /// AND: all sub-expressions must match.
    And(Vec<BooleanExpr>),
    /// OR: any sub-expression must match.
    Or(Vec<BooleanExpr>),
    /// NOT: sub-expression must not match.
    Not(Box<BooleanExpr>),
}

impl BooleanExpr {
    /// Parse a boolean expression string.
    ///
    /// Supports: `AND`, `OR`, `NOT`, parentheses for grouping.
    /// Unquoted patterns are trimmed of whitespace.
    ///
    /// # Examples
    ///
    /// ```
    /// use orange_regex::BooleanExpr;
    ///
    /// let expr = BooleanExpr::parse("error AND NOT warning").unwrap();
    /// let expr = BooleanExpr::parse("(foo OR bar) AND baz").unwrap();
    /// ```
    pub fn parse(input: &str) -> Result<Self, RegexError> {
        let tokens = tokenize(input)?;
        if tokens.is_empty() {
            return Err(RegexError::InvalidExpression("Empty expression".into()));
        }
        let mut parser = Parser::new(&tokens);
        let expr = parser.parse_expr()?;
        if parser.pos < parser.tokens.len() {
            return Err(RegexError::InvalidExpression(format!(
                "Unexpected token: {:?}",
                parser.tokens[parser.pos]
            )));
        }
        Ok(expr)
    }

    /// Evaluate this expression against a set of match results.
    ///
    /// `matched_ids` contains the set of pattern IDs that matched.
    /// Returns true if the boolean expression evaluates to true.
    pub fn evaluate(&self, matched_ids: &[u32]) -> bool {
        match self {
            BooleanExpr::Pattern(_) => {
                // Patterns are evaluated externally; this is a leaf placeholder.
                // Use evaluate_with_mapping for actual evaluation.
                false
            }
            BooleanExpr::And(exprs) => exprs.iter().all(|e| e.evaluate(matched_ids)),
            BooleanExpr::Or(exprs) => exprs.iter().any(|e| e.evaluate(matched_ids)),
            BooleanExpr::Not(inner) => !inner.evaluate(matched_ids),
        }
    }

    /// Collect all leaf pattern strings from this expression.
    pub fn patterns(&self) -> Vec<&str> {
        let mut result = Vec::new();
        self.collect_patterns(&mut result);
        result
    }

    fn collect_patterns<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            BooleanExpr::Pattern(p) => out.push(p),
            BooleanExpr::And(exprs) | BooleanExpr::Or(exprs) => {
                for e in exprs {
                    e.collect_patterns(out);
                }
            }
            BooleanExpr::Not(inner) => inner.collect_patterns(out),
        }
    }
}

// ─── Tokenizer ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    And,
    Or,
    Not,
    LParen,
    RParen,
    Pattern(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>, RegexError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        match ch {
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            _ => {
                // Read a word (non-whitespace, non-paren)
                let mut word = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_whitespace() || ch == '(' || ch == ')' {
                        break;
                    }
                    word.push(ch);
                    chars.next();
                }
                match word.as_str() {
                    "AND" => tokens.push(Token::And),
                    "OR" => tokens.push(Token::Or),
                    "NOT" => tokens.push(Token::Not),
                    _ => tokens.push(Token::Pattern(word)),
                }
            }
        }
    }

    Ok(tokens)
}

// ─── Parser ──────────────────────────────────────────────────────────

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    /// expr → and_expr (OR and_expr)*
    fn parse_expr(&mut self) -> Result<BooleanExpr, RegexError> {
        let mut left = self.parse_and_expr()?;

        while self.peek() == Some(&Token::Or) {
            self.advance(); // consume OR
            let right = self.parse_and_expr()?;
            // Flatten: if left is already Or, extend it
            match left {
                BooleanExpr::Or(mut v) => {
                    v.push(right);
                    left = BooleanExpr::Or(v);
                }
                _ => left = BooleanExpr::Or(vec![left, right]),
            }
        }

        Ok(left)
    }

    /// and_expr → not_expr (AND not_expr)*
    fn parse_and_expr(&mut self) -> Result<BooleanExpr, RegexError> {
        let mut left = self.parse_not_expr()?;

        while self.peek() == Some(&Token::And) {
            self.advance(); // consume AND
            let right = self.parse_not_expr()?;
            // Flatten: if left is already And, extend it
            match left {
                BooleanExpr::And(mut v) => {
                    v.push(right);
                    left = BooleanExpr::And(v);
                }
                _ => left = BooleanExpr::And(vec![left, right]),
            }
        }

        Ok(left)
    }

    /// not_expr → NOT not_expr | atom
    fn parse_not_expr(&mut self) -> Result<BooleanExpr, RegexError> {
        if self.peek() == Some(&Token::Not) {
            self.advance(); // consume NOT
            let inner = self.parse_not_expr()?;
            return Ok(BooleanExpr::Not(Box::new(inner)));
        }
        self.parse_atom()
    }

    /// atom → '(' expr ')' | PATTERN
    fn parse_atom(&mut self) -> Result<BooleanExpr, RegexError> {
        match self.advance() {
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(expr),
                    _ => Err(RegexError::InvalidExpression(
                        "Expected closing parenthesis".into(),
                    )),
                }
            }
            Some(Token::Pattern(p)) => Ok(BooleanExpr::Pattern(p.clone())),
            Some(other) => Err(RegexError::InvalidExpression(format!(
                "Unexpected token: {:?}",
                other
            ))),
            None => Err(RegexError::InvalidExpression(
                "Unexpected end of expression".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_pattern() {
        let expr = BooleanExpr::parse("error").unwrap();
        assert_eq!(expr, BooleanExpr::Pattern("error".into()));
    }

    #[test]
    fn test_parse_and() {
        let expr = BooleanExpr::parse("error AND warning").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::And(vec![
                BooleanExpr::Pattern("error".into()),
                BooleanExpr::Pattern("warning".into()),
            ])
        );
    }

    #[test]
    fn test_parse_or() {
        let expr = BooleanExpr::parse("error OR warning").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::Or(vec![
                BooleanExpr::Pattern("error".into()),
                BooleanExpr::Pattern("warning".into()),
            ])
        );
    }

    #[test]
    fn test_parse_not() {
        let expr = BooleanExpr::parse("NOT error").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::Not(Box::new(BooleanExpr::Pattern("error".into())))
        );
    }

    #[test]
    fn test_parse_complex() {
        // Precedence: NOT > AND > OR
        let expr = BooleanExpr::parse("error AND NOT warning OR info").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::Or(vec![
                BooleanExpr::And(vec![
                    BooleanExpr::Pattern("error".into()),
                    BooleanExpr::Not(Box::new(BooleanExpr::Pattern("warning".into()))),
                ]),
                BooleanExpr::Pattern("info".into()),
            ])
        );
    }

    #[test]
    fn test_parse_parentheses() {
        let expr = BooleanExpr::parse("(error OR warning) AND info").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::And(vec![
                BooleanExpr::Or(vec![
                    BooleanExpr::Pattern("error".into()),
                    BooleanExpr::Pattern("warning".into()),
                ]),
                BooleanExpr::Pattern("info".into()),
            ])
        );
    }

    #[test]
    fn test_parse_nested_not() {
        let expr = BooleanExpr::parse("NOT NOT error").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::Not(Box::new(BooleanExpr::Not(Box::new(
                BooleanExpr::Pattern("error".into())
            ))))
        );
    }

    #[test]
    fn test_parse_empty() {
        assert!(BooleanExpr::parse("").is_err());
        assert!(BooleanExpr::parse("   ").is_err());
    }

    #[test]
    fn test_parse_unbalanced_parens() {
        assert!(BooleanExpr::parse("(error").is_err());
        assert!(BooleanExpr::parse("error)").is_err());
    }

    #[test]
    fn test_parse_double_and() {
        assert!(BooleanExpr::parse("error AND AND warning").is_err());
    }

    #[test]
    fn test_patterns_extraction() {
        let expr = BooleanExpr::parse("(error OR warning) AND NOT info").unwrap();
        let patterns = expr.patterns();
        assert_eq!(patterns, vec!["error", "warning", "info"]);
    }

    #[test]
    fn test_parse_regex_in_patterns() {
        let expr = BooleanExpr::parse(r"\d{4}-\d{2}-\d{2} AND error").unwrap();
        assert_eq!(
            expr,
            BooleanExpr::And(vec![
                BooleanExpr::Pattern(r"\d{4}-\d{2}-\d{2}".into()),
                BooleanExpr::Pattern("error".into()),
            ])
        );
    }
}
