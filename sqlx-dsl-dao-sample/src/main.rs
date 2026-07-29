use sqlx::sqlite::SqlitePoolOptions;
use sqlx_context::DbContext;

// 生成代码里裸路径引用的 `sqlx_context::DbContext` 就是 crates.io 上发布的这个 sqlx-context crate：
// 它把 SqlitePool 和事务统一封装成一个类型，`&mut DbContext` 直接实现了 sqlx::Executor，
// 与生成代码 `ctx: &mut sqlx_context::DbContext` 的调用方式完全匹配。
// 因为它是一个真正的外部crate，Rust 2018+ 的 extern prelude 让它在任意嵌套模块里都能
// 直接写 `sqlx_context::DbContext`，不需要 `extern crate self as ...` 之类的技巧。

// build.rs 在编译前完成了以下工作：
// 1. 创建/重建示例数据库 sample.db，并执行 schema.sql 建表；
// 2. 读取 user/tag 两张表的结构，生成基础CRUD代码；
// 3. 解析 dsl/user.sql 中的DSL函数，生成动态SQL代码；
// 4. 把上述生成结果统一写入 OUT_DIR/dao.rs。
//
// include! 必须放在 crate 根：生成代码里通过 `super::PageIn` 引用分页结构体，
// 一旦嵌套进其它 mod 里 `super` 就指向错误的模块了。
include!(concat!(env!("OUT_DIR"), "/dao.rs"));

#[async_std::main]
async fn main() -> Result<(), sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .connect(env!("DATABASE_URL"))
        .await?;
    let mut ctx: DbContext = DbContext::new(pool);

    // ------------------------- 基础CRUD（根据表结构自动生成）-------------------------

    let alice_id = user_dao::insert(
        &mut ctx,
        &user_dao::User {
            name: "Alice".to_string(),
            email: "alice@example.com".to_string(),
            age: Some(28),
            ..Default::default()
        },
    )
    .await?;
    let bob_id = user_dao::insert(
        &mut ctx,
        &user_dao::User {
            name: "Bob".to_string(),
            email: "bob@example.com".to_string(),
            age: Some(35),
            ..Default::default()
        },
    )
    .await?;
    let carol_id = user_dao::insert(
        &mut ctx,
        &user_dao::User {
            name: "Carol".to_string(),
            email: "carol@example.com".to_string(),
            age: Some(22),
            ..Default::default()
        },
    )
    .await?;
    println!("插入用户: alice={alice_id}, bob={bob_id}, carol={carol_id}");

    let alice = user_dao::select_one(&mut ctx, alice_id).await?;
    println!("查询单条: {:?}", alice);

    let all_users = user_dao::select_all(&mut ctx).await?;
    println!("查询全部（未删除）: {} 条", all_users.len());

    // 整体更新（表里有version列，update内部自动做乐观锁：WHERE ... AND version = ?）
    let mut alice = alice;
    alice.age = Some(29);
    let updated = user_dao::update(&mut ctx, &alice).await?;
    println!("整体更新影响行数: {updated}");

    // ------------------------- 动态SQL（根据 dsl/user.sql 生成）-------------------------

    // -- @if 条件拼接：按姓名模糊 + 最小年龄过滤
    let matched =
        user_dao::find_user_by_condition(&mut ctx, Some("a".to_string()), Some(25)).await?;
    println!("按条件查询(name含a且age>=25): {} 条", matched.len());

    // -- @each 循环拼接：IN 查询
    let by_ids = user_dao::find_user_by_ids(&mut ctx, vec![alice_id, bob_id]).await?;
    println!("按id列表查询: {} 条", by_ids.len());

    // 标量返回值（i64）
    let total = user_dao::count_user(&mut ctx).await?;
    println!("用户总数: {total}");

    // -- @page 分页查询
    let page_in = PageIn {
        page: 1,
        page_size: 2,
        sort_key: "id".to_string(),
        sort_type: "desc".to_string(),
    };
    let (count, page_data) = user_dao::page_user(&mut ctx, page_in, None).await?;
    println!("分页查询: 总数={count}, 本页={} 条", page_data.len());

    // 动态UPDATE：只更新调用时实际传入的字段
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let affected = user_dao::update_user_profile(&mut ctx, bob_id, now, None, Some(36)).await?;
    println!("动态更新影响行数: {affected}");

    // ------------------------- 删除 -------------------------

    let carol = user_dao::select_one(&mut ctx, carol_id).await?;
    user_dao::set_delete(&mut ctx, carol_id, carol.version, "admin".to_string()).await?;
    println!("逻辑删除 carol 完成");

    let after_delete = user_dao::select_all(&mut ctx).await?;
    println!("逻辑删除后查询全部（未删除）: {} 条", after_delete.len());

    let all_include_deleted = user_dao::select_all_include_deleted(&mut ctx).await?;
    println!("查询全部（含已删除）: {} 条", all_include_deleted.len());

    user_dao::delete(&mut ctx, carol_id).await?;
    println!("物理删除 carol 完成");

    // ------------------------- 简单表（没有deleted/version列，只生成基础CRUD）-------------------------

    let tag_id = tag_dao::insert(
        &mut ctx,
        &tag_dao::Tag {
            name: "rust".to_string(),
            ..Default::default()
        },
    )
    .await?;
    let mut tag = tag_dao::select_one(&mut ctx, tag_id).await?;
    tag.name = "rust-lang".to_string();
    tag_dao::update(&mut ctx, &tag).await?;
    let tags = tag_dao::select_all(&mut ctx).await?;
    println!("标签: {:?}", tags);
    tag_dao::delete(&mut ctx, tag_id).await?;

    Ok(())
}
