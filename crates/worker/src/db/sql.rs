use domain::ids::Timestamp;
use worker::js_sys::Uint8Array;
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, D1PreparedStatement, D1Result, Error, Result};

/// D1 hands numbers to JavaScript, which is exact only up to 2^53.
const MAX_EXACT_INT: i64 = (1 << 53) - 1;

#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    Text(String),
    Int(i64),
    Real(f64),
    Null,
    Blob(Vec<u8>),
}

impl Param {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    pub fn opt_text(value: Option<impl Into<String>>) -> Self {
        value.map_or(Self::Null, |v| Self::Text(v.into()))
    }

    pub fn flag(value: bool) -> Self {
        Self::Int(i64::from(value))
    }

    pub fn ts(value: Timestamp) -> Self {
        Self::Int(value.0)
    }

    pub fn opt_ts(value: Option<Timestamp>) -> Self {
        value.map_or(Self::Null, Self::ts)
    }

    fn to_js(&self) -> Result<JsValue> {
        Ok(match self {
            Self::Text(s) => JsValue::from_str(s),
            Self::Int(i) if i.unsigned_abs() > MAX_EXACT_INT.unsigned_abs() => {
                return Err(Error::RustError(format!("integer {i} is not exact in D1")))
            }
            Self::Int(i) => JsValue::from_f64(*i as f64),
            Self::Real(f) => JsValue::from_f64(*f),
            Self::Null => JsValue::NULL,
            Self::Blob(bytes) => Uint8Array::from(bytes.as_slice()).into(),
        })
    }
}

/// One parameterized statement. Upserts return this and never touch D1
/// themselves, so tests run the same statements against SQLite natively.
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub sql: &'static str,
    pub params: Vec<Param>,
}

impl Statement {
    pub fn prepare(&self, db: &D1Database) -> Result<D1PreparedStatement> {
        let args = self
            .params
            .iter()
            .map(Param::to_js)
            .collect::<Result<Vec<_>>>()?;
        db.prepare(self.sql).bind(&args)
    }
}

/// Sends the statements as one D1 batch, which D1 runs as a single transaction.
pub async fn run(db: &D1Database, statements: Vec<Statement>) -> Result<Vec<D1Result>> {
    if statements.is_empty() {
        return Ok(Vec::new());
    }
    let prepared = statements
        .iter()
        .map(|s| s.prepare(db))
        .collect::<Result<Vec<_>>>()?;
    db.batch(prepared).await
}

#[cfg(any(test, feature = "sql-dump"))]
mod literal {
    use super::{Param, Statement};
    use std::fmt::{self, Write as _};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum LiteralError {
        MissingParam(usize),
        NonFiniteReal,
    }

    impl fmt::Display for LiteralError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::MissingParam(n) => write!(f, "statement uses ?{n} but has no such parameter"),
                Self::NonFiniteReal => f.write_str("a real parameter is not finite"),
            }
        }
    }

    impl std::error::Error for LiteralError {}

    impl Param {
        fn literal(&self) -> Result<String, LiteralError> {
            Ok(match self {
                Self::Text(s) => text_literal(s),
                Self::Int(i) => i.to_string(),
                Self::Real(f) if f.is_finite() => format!("{f:?}"),
                Self::Real(_) => return Err(LiteralError::NonFiniteReal),
                Self::Null => "NULL".to_owned(),
                Self::Blob(bytes) => {
                    let mut out = String::from("X'");
                    for b in bytes {
                        let _ = write!(out, "{b:02x}");
                    }
                    out.push('\'');
                    out
                }
            })
        }
    }

    /// Control characters become `char(n)` calls so every statement stays on one
    /// line, which is how D1 splits a file into statements.
    fn text_literal(s: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut run = String::new();
        let flush = |run: &mut String, parts: &mut Vec<String>| {
            if !run.is_empty() {
                parts.push(format!("'{}'", run.replace('\'', "''")));
                run.clear();
            }
        };
        for c in s.chars() {
            if c.is_control() && c != '\t' {
                flush(&mut run, &mut parts);
                parts.push(format!("char({})", u32::from(c)));
            } else {
                run.push(c);
            }
        }
        flush(&mut run, &mut parts);
        match parts.len() {
            0 => "''".to_owned(),
            1 => parts.remove(0),
            _ => format!("({})", parts.join(" || ")),
        }
    }

    impl Statement {
        /// The statement with every `?N` replaced by its escaped literal, on one line.
        pub fn to_literal_sql(&self) -> Result<String, LiteralError> {
            let mut out = String::with_capacity(self.sql.len() * 2);
            let mut chars = self.sql.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '?' => {
                        let mut digits = String::new();
                        while let Some(d) = chars.next_if(char::is_ascii_digit) {
                            digits.push(d);
                        }
                        let n: usize = digits.parse().unwrap_or(0);
                        let param = n
                            .checked_sub(1)
                            .and_then(|i| self.params.get(i))
                            .ok_or(LiteralError::MissingParam(n))?;
                        out.push_str(&param.literal()?);
                    }
                    '\n' | '\r' => out.push(' '),
                    _ => out.push(c),
                }
            }
            Ok(format!("{};", out.trim()))
        }
    }
}

#[cfg(any(test, feature = "sql-dump"))]
pub use literal::LiteralError;
