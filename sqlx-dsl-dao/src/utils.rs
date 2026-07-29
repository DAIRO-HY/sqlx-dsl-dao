
// hello_word -> HelloWord
pub fn snake_to_pascal(s: impl AsRef<str>, sep: &str) -> String {
    s.as_ref().split(sep)
        .filter(|part| !part.is_empty()) // 跳过连续下划线导致的空段
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<String>()
}
