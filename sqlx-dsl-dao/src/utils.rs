
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

/// 生成写SQL执行成功后记录写日志的代码,未开启写日志时只多一次判断
/// `sql`: SQL字符串表达式(字面量或变量); `args`: 按绑定顺序排列的参数表达式
pub fn write_log_src(sql: &str, args: &[String]) -> String {
    let values = args
        .iter()
        .map(|it| format!("sqlx_context::log_value(&({}))", it))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "if ctx.is_write_log_enabled() {{\n    ctx.push_write_log({}, vec![{}]);\n}}\n",
        sql, values
    )
}
