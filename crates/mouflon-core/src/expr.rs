//! Expressions numériques des fichiers desktopPet : nombres, variables
//! (`screenW`, `imageW`, `random`…), opérateurs `+ - * / %`, parenthèses et
//! `Convert(x, System.Int32)`.
//!
//! Les règles suivent le moteur d'expressions .NET utilisé par desktopPet :
//! `/` divise en virgule flottante, `%` ne travaille que sur des entiers et
//! lie moins fort que `*` et `/`, `Convert` arrondit au plus proche (égalité
//! vers le pair).
//!
//! L'analyse se fait une fois au chargement ; l'évaluation demande la valeur
//! des variables à une fonction fournie par l'appelant.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Num(f64),
    Var(String),
    Neg(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    /// `Convert(x, System.Int32)` : arrondi au plus proche, égalité vers le pair.
    Round(Box<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// Expression analysée.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    source: String,
    root: Node,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("expression « {expression} » invalide : {message}")]
pub struct ExprError {
    pub expression: String,
    pub message: String,
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

impl Expr {
    /// Analyse une expression.
    pub fn parse(source: &str) -> Result<Self, ExprError> {
        let mut parser = Parser {
            chars: source.chars().collect(),
            pos: 0,
            source,
        };
        let root = parser.expr()?;
        parser.skip_ws();
        if parser.pos < parser.chars.len() {
            return Err(parser.error(&format!(
                "caractère inattendu « {} »",
                parser.chars[parser.pos]
            )));
        }
        Ok(Self {
            source: source.trim().to_owned(),
            root,
        })
    }

    /// Expression constante.
    #[must_use]
    pub fn constant(value: f64) -> Self {
        Self {
            source: value.to_string(),
            root: Node::Num(value),
        }
    }

    /// Valeur si l'expression ne dépend d'aucune variable.
    #[must_use]
    pub fn as_constant(&self) -> Option<f64> {
        fn walk(n: &Node) -> Option<f64> {
            Some(match n {
                Node::Num(v) => *v,
                Node::Var(_) => return None,
                Node::Neg(a) => -walk(a)?,
                Node::Bin(op, a, b) => apply(*op, walk(a)?, walk(b)?),
                Node::Round(a) => walk(a)?.round_ties_even(),
            })
        }
        walk(&self.root)
    }

    /// Noms des variables utilisées.
    #[must_use]
    pub fn variables(&self) -> Vec<&str> {
        fn walk<'a>(n: &'a Node, out: &mut Vec<&'a str>) {
            match n {
                Node::Num(_) => {}
                Node::Var(v) => {
                    if !out.contains(&v.as_str()) {
                        out.push(v);
                    }
                }
                Node::Neg(a) | Node::Round(a) => walk(a, out),
                Node::Bin(_, a, b) => {
                    walk(a, out);
                    walk(b, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    /// Évalue l'expression ; une variable inconnue vaut 0.
    pub fn eval(&self, vars: &mut dyn FnMut(&str) -> Option<f64>) -> f64 {
        fn walk(n: &Node, vars: &mut dyn FnMut(&str) -> Option<f64>) -> f64 {
            match n {
                Node::Num(v) => *v,
                Node::Var(name) => vars(name).unwrap_or(0.0),
                Node::Neg(a) => -walk(a, vars),
                Node::Bin(op, a, b) => {
                    let x = walk(a, vars);
                    let y = walk(b, vars);
                    apply(*op, x, y)
                }
                Node::Round(a) => walk(a, vars).round_ties_even(),
            }
        }
        let v = walk(&self.root, vars);
        if v.is_finite() { v } else { 0.0 }
    }
}

fn apply(op: Op, x: f64, y: f64) -> f64 {
    match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => {
            if y == 0.0 {
                0.0
            } else {
                x / y
            }
        }
        // Reste entier, du signe du dividende (comme en C#).
        Op::Rem => {
            let (x, y) = (x.trunc(), y.trunc());
            if y == 0.0 { 0.0 } else { x % y }
        }
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    source: &'a str,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> ExprError {
        ExprError {
            expression: self.source.to_owned(),
            message: message.to_owned(),
        }
    }

    fn skip_ws(&mut self) {
        while self.chars.get(self.pos).is_some_and(|c| c.is_whitespace()) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.chars.get(self.pos).copied()
    }

    fn expr(&mut self) -> Result<Node, ExprError> {
        let mut left = self.modulo()?;
        while let Some(c) = self.peek() {
            let op = match c {
                '+' => Op::Add,
                '-' => Op::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.modulo()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    /// `%` lie moins fort que `*` et `/` (règle .NET) : `a % b * c` = `a % (b * c)`.
    fn modulo(&mut self) -> Result<Node, ExprError> {
        let mut left = self.term()?;
        while self.peek() == Some('%') {
            self.pos += 1;
            let right = self.term()?;
            left = Node::Bin(Op::Rem, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Node, ExprError> {
        let mut left = self.unary()?;
        while let Some(c) = self.peek() {
            let op = match c {
                '*' => Op::Mul,
                '/' => Op::Div,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        match self.peek() {
            Some('-') => {
                self.pos += 1;
                Ok(Node::Neg(Box::new(self.unary()?)))
            }
            Some('+') => {
                self.pos += 1;
                self.unary()
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Result<Node, ExprError> {
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let inner = self.expr()?;
                if self.peek() != Some(')') {
                    return Err(self.error("parenthèse fermante manquante"));
                }
                self.pos += 1;
                Ok(inner)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.pos;
                while self
                    .chars
                    .get(self.pos)
                    .is_some_and(|c| c.is_ascii_digit() || *c == '.')
                {
                    self.pos += 1;
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                text.parse::<f64>()
                    .map(Node::Num)
                    .map_err(|_| self.error(&format!("nombre invalide « {text} »")))
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                let start = self.pos;
                while self
                    .chars
                    .get(self.pos)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                {
                    self.pos += 1;
                }
                let name: String = self.chars[start..self.pos].iter().collect();
                if self.peek() != Some('(') {
                    return Ok(Node::Var(name));
                }
                self.pos += 1;
                // `Math.random()` et consorts : des parenthèses vides gardent le nom.
                if self.peek() == Some(')') {
                    self.pos += 1;
                    return Ok(Node::Var(name));
                }
                if !name.eq_ignore_ascii_case("Convert") {
                    return Err(self.error(&format!("fonction inconnue « {name} »")));
                }
                // `Convert(valeur, System.Int32)` : seul le type entier est utilisé par les animaux.
                let value = self.expr()?;
                if self.peek() == Some(',') {
                    self.pos += 1;
                    self.skip_ws();
                    while self
                        .chars
                        .get(self.pos)
                        .is_some_and(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
                    {
                        self.pos += 1;
                    }
                }
                if self.peek() != Some(')') {
                    return Err(self.error("parenthèse fermante manquante après Convert"));
                }
                self.pos += 1;
                Ok(Node::Round(Box::new(value)))
            }
            Some(c) => Err(self.error(&format!("caractère inattendu « {c} »"))),
            None => Err(self.error("expression incomplète")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str) -> f64 {
        Expr::parse(src).unwrap().eval(&mut |name| match name {
            "screenW" => Some(1440.0),
            "imageW" => Some(40.0),
            "random" => Some(50.0),
            _ => None,
        })
    }

    #[test]
    fn precedence_and_parentheses() {
        assert_eq!(eval("1 + 2 * 3"), 7.0);
        assert_eq!(eval("(1 + 2) * 3"), 9.0);
        assert_eq!(eval("-4 + 10 / 4"), -1.5);
        assert_eq!(eval("7 % 4"), 3.0);
    }

    #[test]
    fn variables_and_constants() {
        assert_eq!(eval("screenW - imageW"), 1400.0);
        assert_eq!(eval("screenW*random/100"), 720.0);
        assert_eq!(eval("unknown + 1"), 1.0);
        assert_eq!(Expr::parse("3*4").unwrap().as_constant(), Some(12.0));
        assert_eq!(Expr::parse("imageW*2").unwrap().as_constant(), None);
        assert_eq!(Expr::parse("a + b*a").unwrap().variables(), vec!["a", "b"]);
    }

    #[test]
    fn division_by_zero_is_harmless() {
        assert_eq!(eval("5 / 0"), 0.0);
        assert_eq!(eval("5 % 0"), 0.0);
    }

    #[test]
    fn dotnet_rules() {
        // `/` en virgule flottante, `%` entier et moins prioritaire que `*` et `/`.
        assert_eq!(eval("random/20"), 2.5);
        assert_eq!(eval("7 % 2 * 3"), 1.0);
        assert_eq!(eval("2 * 7 % 4"), 2.0);
        assert_eq!(eval("-7 % 4"), -3.0);
        // Convert arrondit au plus proche, égalité vers le pair.
        assert_eq!(eval("Convert(682.5, System.Int32)"), 682.0);
        assert_eq!(eval("Convert(683.5,System.Int32)"), 684.0);
        assert_eq!(eval("25+(Convert(screenW/2,System.Int32)%30)/7"), 25.0);
        assert!(Expr::parse("IIF(1, 2, 3)").is_err());
    }

    #[test]
    fn reports_errors() {
        assert!(Expr::parse("(1 + 2").is_err());
        assert!(Expr::parse("1 +").is_err());
        assert!(Expr::parse("2 $ 3").is_err());
    }
}
