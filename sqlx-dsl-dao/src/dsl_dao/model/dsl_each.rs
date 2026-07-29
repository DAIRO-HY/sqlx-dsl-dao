/// IF条件
#[derive(Debug, serde::Serialize, Default, Clone)]
pub struct DslEach {
    pub name: String,    //循环的变量名
    pub seq: String,     //分隔符
    pub open: String,    //开口字符
    pub close: String,   //闭口字符
    pub item: String,    //内部变量名
}
