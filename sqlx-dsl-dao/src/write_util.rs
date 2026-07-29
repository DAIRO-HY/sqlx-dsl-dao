use crate::table_dao::model::table;
use std::collections::HashMap;
use std::{env, fs};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

/// 共通部分的代码
const MOD_COMMON_SRC: &'static str = r##"
pub struct PageIn {
    /// 当前页
    pub(crate) page: u32,

    /// 页面显示件数
    pub(crate) page_size: u32,

    /// 排序字段
    pub(crate) sort_key: String,

    /// 排序方式
    pub(crate) sort_type: String,
}
impl PageIn {

    /// 验证排序是否合法
    pub fn check_order_by(&self) -> Result<(), sqlx::Error> {
        for ch in self.sort_key.chars() {
            if !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == ',' || ch == ' ') {
                return Err(sqlx::Error::InvalidArgument("排序方式中存在非法字符".to_string()));
            }
        }
        match self.sort_type.to_lowercase().as_str() {
            "asc" | "desc" | "" => {}
            _ => {
                return Err(sqlx::Error::InvalidArgument("排序方式中存在非法字符".to_string()));
            }
        }
        Ok(())
    }
}
    "##;

/// 用来存储准备生成的Dao代码
static METHOD_DAO_SOURCE_PREPARE: LazyLock<Mutex<HashMap<String, MethodSource>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 用来存储准备生成的Dao的Entity
/// 文件名 -> Entity名 -> Entity
static METHOD_DAO_ENTITY_PREPARE: LazyLock<Mutex<HashMap<String, HashMap<String, table::Entity>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 添加准备写入的函数代码
pub fn add_method_source(mod_name: &str, src: String) {
    let mut source_map = METHOD_DAO_SOURCE_PREPARE.lock().unwrap();
    if source_map.contains_key(mod_name) {
        //如果文件已经存在,则追加
        source_map
            .get_mut(mod_name)
            .unwrap()
            .src
            .push_str(src.as_str());
    } else {
        source_map.insert(
            mod_name.to_string(),
            MethodSource {
                mod_name: mod_name.to_string(),
                src,
            },
        );
    }
}

/// 添加准备写入的Entity
pub fn add_entity(mod_name: &str, table: table::Entity) {
    let mut entity_map = METHOD_DAO_ENTITY_PREPARE.lock().unwrap();
    if !entity_map.contains_key(mod_name) {
        entity_map.insert(mod_name.to_string(), HashMap::new());
    }
    let map = entity_map.get_mut(mod_name).unwrap();
    let entity_name = table.entity_name();

    //若同名Entity已经存在,则跳过
    if !map.contains_key(entity_name.as_str()) {
        map.insert(entity_name, table);
    }
}

/// 写入文件
pub fn write() {
    let source_map = METHOD_DAO_SOURCE_PREPARE.lock().unwrap();
    let entity_file_map = METHOD_DAO_ENTITY_PREPARE.lock().unwrap();
    let mut all_source = format!("// Generated at {}\n", chrono::Local::now().to_rfc3339());

    //导入共通部分代码
    all_source.push_str(MOD_COMMON_SRC);
    source_map.iter().for_each(|(mod_name, method)| {
        let mut source = method.src.clone();
        if let Some(entity_map) = entity_file_map.get(mod_name) {
            entity_map.iter().for_each(|(_, table)| {
                //生成Entity代码
                source.push_str(table.make_entity_struct().as_str());
            });
        }
        all_source.push_str(format!("pub mod {}_dao {{\n{}\n}}\n", mod_name, source).as_str());
    });
    format_and_write(&all_source);
}

fn format_and_write(source: &str) {

    //生成的dao源代码写到 OUT_DIR/dao 目录
    let save_file = Path::new(&(env::var("OUT_DIR").unwrap().as_str())).join("dao.rs");
    // let save_file = Path::new("/Users/zhoulq/dev/java/idea/FarmingServer/Main/src/gen_dao/mod.rs");


    // if !save_path.exists() {
    //     //如果目录不存在则创建
    //     fs::create_dir(&save_path).unwrap();
    // }
    //
    // //写入文件夹
    // let base_dir = "./Main/src/gen_dao";

    // 解析成 syn AST
    let rust_src = syn::parse_str(source);
    match rust_src {
        Err(err) => {
            eprintln!("cargo:warning=解析代码文件出错:{:?}", err);
            // eprintln!("cargo:warning=解析{}文件出错:{}", file_path.display(), it);
            // eprintln!("cargo:warning=文件内容:\n{}", dao_src);
            // fs::write(&file_path, dao_src).unwrap();
            // panic!("文件解析失败");
            fs::write(&save_file, source).unwrap();
        }
        Ok(it) => {
            //使用 prettyplease 格式化
            let formatted_rust_src = prettyplease::unparse(&it);
            fs::write(&save_file, formatted_rust_src).unwrap();
        }
    }
    println!(
        "cargo:warning=Generated DAO source saved to '{}'",
        save_file.display()
    );
}

struct MethodSource {
    mod_name: String,
    src: String,
}
