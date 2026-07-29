
/// IF条件
#[derive(Debug, serde::Serialize, Default, Clone)]
pub struct DslIfCondition {
    pub name: String,                //判断条件变量名
    pub value: String,               //条件值
    pub operator: String,            //运算符:> 、<、<=、>=、== 、!=
    pub next_condition_type: String, //与下一个判断连接方式: && 或 ||
}


impl DslIfCondition {
    /// 获取rust类型的值
    pub fn rust_value(&self) -> &str {
        match self.value.to_lowercase().as_str() {
            "null" => "None",
            _ => self.value.as_str(),
        }
    }

    /// 生成条件渲染的表达式部分的代码.如  key > 0 || name != null
    /// need_param: 是否需要一个param变量
    pub fn condition_src(&self, need_param: bool) -> String {
        let var_name = if need_param {
            format!("param.{}", self.name)
        } else {
            self.name.clone()
        };
        let mut body = String::new();
        body.push_str(&var_name);
        body.push_str(" ");
        body.push_str(&self.operator);
        body.push_str(" ");
        body.push_str(&self.rust_value());
        body.push_str(" ");
        body.push_str(&self.next_condition_type);
        body.push_str(" ");
        body
    }
}