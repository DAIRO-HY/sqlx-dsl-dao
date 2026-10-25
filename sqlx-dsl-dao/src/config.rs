use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

/// 按列名自动识别的约定字段
pub const CONVENTION_COLUMNS: [&str; 6] = [
    "version",
    "created_at",
    "updated_at",
    "deleted",
    "deleted_at",
    "deleted_by",
];

/// 本次生成使用的配置,由 `generate()` 在生成前设置
static CONFIG: LazyLock<Mutex<GenerateConfig>> =
    LazyLock::new(|| Mutex::new(GenerateConfig::default()));

/// [`generate_with_config()`](crate::generate_with_config) 的生成配置
///
/// ```no_run
/// let config = sqlx_dsl_dao::GenerateConfig::new()
///     .type_override("property", "String")
///     .disable_convention("user_token.version") // 只关闭 user_token 表的 version 约定
///     .lowercase_keywords(true);
/// ```
#[derive(Debug, Clone, Default)]
pub struct GenerateConfig {
    /// 字段名 -> Rust类型 的覆盖规则
    pub(crate) type_overrides: HashMap<String, String>,

    /// 关闭的约定:`列名`(全局) / `表名.列名` / `表名.*` / `*`
    pub(crate) disabled_conventions: HashSet<String>,

    /// 表DAO生成的SQL关键字是否使用小写
    pub(crate) lowercase_keywords: bool,
}

impl GenerateConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// 批量添加 字段名 -> Rust类型 的覆盖规则
    pub fn type_overrides(mut self, overrides: &HashMap<String, String>) -> Self {
        self.type_overrides
            .extend(overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
        self
    }

    /// 按列名强制指定Rust类型
    pub fn type_override(mut self, column: impl Into<String>, rust_type: impl Into<String>) -> Self {
        self.type_overrides.insert(column.into(), rust_type.into());
        self
    }

    /// 关闭约定字段(version / created_at / updated_at / deleted / deleted_at / deleted_by)的特殊处理,
    /// 被关闭的列按普通列生成代码。可以写:
    /// - `"version"`:所有表的 version 列
    /// - `"user_token.version"`:只针对 user_token 表的 version 列
    /// - `"user_token.*"`:user_token 表的全部约定
    /// - `"*"`:所有表的全部约定
    pub fn disable_convention(mut self, rule: impl AsRef<str>) -> Self {
        let rule = rule.as_ref().trim();
        let column = rule.rsplit('.').next().unwrap();
        if column != "*" && !CONVENTION_COLUMNS.contains(&column) {
            panic!(
                "disable_convention(\"{}\"):列名必须是 {:?} 之一或 *",
                rule, CONVENTION_COLUMNS
            );
        }
        self.disabled_conventions.insert(rule.to_string());
        self
    }

    /// 表DAO生成的SQL关键字使用小写(`insert into` / `update` / `delete from` / `select` 等),默认大写。
    /// DSL文件里手写的SQL不受影响,始终原样保留。
    pub fn lowercase_keywords(mut self, enable: bool) -> Self {
        self.lowercase_keywords = enable;
        self
    }

    /// 某张表的某个约定字段是否启用
    fn convention_enabled(&self, table: &str, column: &str) -> bool {
        let rules = &self.disabled_conventions;
        !(rules.contains("*")
            || rules.contains(column)
            || rules.contains(&format!("{}.*", table))
            || rules.contains(&format!("{}.{}", table, column)))
    }
}

/// 设置本次生成使用的配置
pub(crate) fn set(config: &GenerateConfig) {
    *CONFIG.lock().unwrap() = config.clone();
}

/// 按列名获取覆盖的Rust类型
pub(crate) fn type_override(column: &str) -> Option<String> {
    CONFIG.lock().unwrap().type_overrides.get(column).cloned()
}

/// 某张表的某个约定字段是否启用
pub(crate) fn convention_enabled(table: &str, column: &str) -> bool {
    CONFIG.lock().unwrap().convention_enabled(table, column)
}

/// 按配置输出SQL关键字的大小写,关键字以大写形式传入
pub(crate) fn kw(keyword: &str) -> String {
    if CONFIG.lock().unwrap().lowercase_keywords {
        keyword.to_lowercase()
    } else {
        keyword.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_convention_rules() {
        let config = GenerateConfig::new()
            .disable_convention("user_token.version")
            .disable_convention("created_at")
            .disable_convention("log.*");
        assert!(!config.convention_enabled("user_token", "version"));
        assert!(config.convention_enabled("user", "version"));
        assert!(!config.convention_enabled("user", "created_at"));
        assert!(!config.convention_enabled("log", "deleted"));
        assert!(config.convention_enabled("user", "deleted"));
        assert!(!GenerateConfig::new().disable_convention("*").convention_enabled("a", "deleted"));
    }

    #[test]
    #[should_panic]
    fn disable_convention_rejects_unknown_column() {
        GenerateConfig::new().disable_convention("user.name");
    }
}
