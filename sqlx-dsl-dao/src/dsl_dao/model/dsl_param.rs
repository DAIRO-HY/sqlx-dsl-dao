
/// 书库操作函数参数信息
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct DslParam {
    /// 参数名
    pub name: String,

    /// 参数类型
    pub ty: String,

    /// 注释
    pub comment: String,

    /// 是否允许Null
    pub is_option: bool,
}

impl DslParam {
    /// 获取rust类型
    pub fn rust_type(&self) -> String {
        let ty = match self.ty.as_str() {
            _ => self.ty.as_str(),
        };
        if self.is_option{
            format!("Option<{}>", ty)
        } else{
            ty.to_string()
        }
    }

    /// 获取定义变量代码
    pub fn get_define_src() {}
}