use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::iter;
use std::ops::Not;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Label(SmallVec<[SmolStr; 8]>);

impl Label {
    pub fn insert<S: AsRef<str>>(&mut self, s: S) {
        if self.0.iter().any(|i| i == s.as_ref()) {
            return;
        }

        self.0.push(SmolStr::new(s));
    }

    pub fn remove(&mut self, s: &str) {
        self.0.retain(|i| i != s);
    }

    pub fn all<'a, I: IntoIterator<Item = &'a str>>(&self, sub: I) -> bool {
        for s in sub {
            if !self.0.iter().any(|i| i == s) {
                return false;
            }
        }
        true
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|i| i.as_str())
    }
}

impl<T, S> From<T> for Label
where
    T: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    fn from(it: T) -> Self {
        let mut label = Self::default();

        for s in it {
            label.insert(s);
        }

        label
    }
}

/// A helper macro to build `Label`, e.g. `label!("network")`.
#[macro_export]
macro_rules! label {
    ($($x: expr),* $(,)?) => {
        {
            #[allow(unused_mut)]
            let mut label = $crate::Label::default();
            $(label.insert(&$x);)*
            label
        }
    };
}

/// Route matching operations, e.g.
/// ```rust
/// ipmb::LabelOp::from("foo").and("bar").or(!ipmb::LabelOp::from("baz"));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LabelOp {
    True,
    False,
    Leaf(SmolStr),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl LabelOp {
    pub fn and(self, v: impl Into<Self>) -> Self {
        Self::And(Box::new(self), Box::new(v.into()))
    }

    pub fn or(self, v: impl Into<Self>) -> Self {
        Self::Or(Box::new(self), Box::new(v.into()))
    }

    pub fn validate(&self, label: &Label) -> bool {
        match self {
            Self::True => true,
            Self::False => false,
            Self::Leaf(v) => label.all(iter::once(v.as_str())),
            Self::And(left, right) => left.validate(label) && right.validate(label),
            Self::Or(left, right) => left.validate(label) || right.validate(label),
            Self::Not(left) => !left.validate(label),
        }
    }
}

impl Not for LabelOp {
    type Output = Self;

    fn not(self) -> Self::Output {
        Self::Not(Box::new(self))
    }
}

impl<T: Into<SmolStr>> From<T> for LabelOp {
    fn from(value: T) -> Self {
        LabelOp::Leaf(value.into())
    }
}

#[cfg(test)]
mod test {
    use super::LabelOp;
    use crate::label;

    #[test]
    fn empty_label() {
        let _ = label!();
    }

    #[test]
    fn test_label() {
        let earth = "earth".to_string();
        let moon = "moon".to_string();
        let mut label = label!("solar", earth, moon);
        assert!(label.all(["solar", "earth", "moon"]));

        label.remove("moon");
        assert!(!label.all(["moon"]));
    }

    #[test]
    fn op_true() {
        let op = LabelOp::True;
        assert!(op.validate(&label!("foo")));
    }

    #[test]
    fn op_false() {
        let op = LabelOp::False;
        assert!(!op.validate(&label!("foo")));
    }

    #[test]
    fn op_leaf() {
        let op = LabelOp::from("foo");
        assert!(op.validate(&label!("foo")));
        assert!(!op.validate(&label!("bar")));
    }

    #[test]
    fn op_not() {
        let op = !LabelOp::from("foo");
        assert!(op.validate(&label!("bar", "baz")));
    }

    #[test]
    fn op_and() {
        let op = LabelOp::from("foo").and("bar");
        assert!(op.validate(&label!("foo", "bar", "baz")));
    }

    #[test]
    fn op_or() {
        let op = LabelOp::from("foo").or("bar");
        assert!(op.validate(&label!("foo")));
        assert!(op.validate(&label!("bar")));
    }
}
