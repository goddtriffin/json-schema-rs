#[must_use]
pub fn temp(num: i32) -> i32 {
    num + num
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temp() {
        assert_eq!(temp(1), 2);
    }
}
