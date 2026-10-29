//! A non-empty list of errors, one per problem found.

use std::fmt;
use std::ops::Deref;

/// Every problem one operation found, each as its own error, in the order they were found
/// (deterministic: the same input always lists the same errors in the same order). Never empty.
///
/// Parsing and registry building do not stop at the first problem, so a caller can fix them all
/// in one pass. Unlike `Vec<E>`, this implements [`std::error::Error`], so it works with `?` and
/// `Box<dyn Error>`. [`Display`](fmt::Display) prints one error per line. It derefs to `[E]` and
/// iterates over the errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorList<E> {
    errors: Vec<E>,
}

impl<E> ErrorList<E> {
    /// A list holding one error.
    #[must_use]
    pub fn new(error: E) -> Self {
        ErrorList {
            errors: vec![error],
        }
    }

    /// A list of `errors`, or `None` when there are none.
    #[must_use]
    pub fn from_vec(errors: Vec<E>) -> Option<Self> {
        (!errors.is_empty()).then_some(ErrorList { errors })
    }

    /// The first error found.
    #[must_use]
    pub fn first(&self) -> &E {
        &self.errors[0]
    }

    /// The errors, in the order they were found.
    #[must_use]
    pub fn into_vec(self) -> Vec<E> {
        self.errors
    }
}

impl<E> Deref for ErrorList<E> {
    type Target = [E];

    fn deref(&self) -> &[E] {
        &self.errors
    }
}

impl<E> From<E> for ErrorList<E> {
    fn from(error: E) -> Self {
        ErrorList::new(error)
    }
}

impl<E> IntoIterator for ErrorList<E> {
    type Item = E;
    type IntoIter = std::vec::IntoIter<E>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.into_iter()
    }
}

impl<'l, E> IntoIterator for &'l ErrorList<E> {
    type Item = &'l E;
    type IntoIter = std::slice::Iter<'l, E>;

    fn into_iter(self) -> Self::IntoIter {
        self.errors.iter()
    }
}

impl<E: fmt::Display> fmt::Display for ErrorList<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.errors.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{error}")?;
        }
        Ok(())
    }
}

impl<E: std::error::Error> std::error::Error for ErrorList<E> {}

#[cfg(test)]
mod tests {
    use super::ErrorList;

    #[test]
    fn from_vec_of_none_is_none() {
        let expected: Option<ErrorList<String>> = None;
        let actual: Option<ErrorList<String>> = ErrorList::from_vec(Vec::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn from_vec_keeps_order() {
        let expected: Vec<String> = vec!["b".to_string(), "a".to_string()];
        let actual: Vec<String> = ErrorList::from_vec(vec!["b".to_string(), "a".to_string()])
            .unwrap()
            .into_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_is_the_first_error_found() {
        let list: ErrorList<String> =
            ErrorList::from_vec(vec!["b".to_string(), "a".to_string()]).unwrap();
        let expected: &str = "b";
        let actual: &str = list.first();
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_prints_one_error_per_line() {
        let list: ErrorList<String> =
            ErrorList::from_vec(vec!["first".to_string(), "second".to_string()]).unwrap();
        let expected: &str = "first\nsecond";
        let actual: String = list.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_of_one_error_has_no_newline() {
        let list: ErrorList<String> = ErrorList::new("only".to_string());
        let expected: &str = "only";
        let actual: String = list.to_string();
        assert_eq!(expected, actual);
    }
}
