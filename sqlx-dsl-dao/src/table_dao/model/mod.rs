use serde::Serialize;

pub mod table;

/// sql文的CRUD类型
#[derive(PartialEq, Debug, Default, Clone,Serialize)]
pub enum CrudType {
    #[default]
    Unknown,
    Create,//插入
    Read,//读取
    Update,//更新
    Delete,//删除
}