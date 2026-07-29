-- 用户表：演示完整特性（自增主键、乐观锁 version、逻辑删除 deleted/deleted_at/deleted_by）
CREATE TABLE IF NOT EXISTS user ( -- 用户
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- 主键
    name VARCHAR NOT NULL, -- 用户名
    email VARCHAR NOT NULL, -- 邮箱
    age INTEGER, -- 年龄，可为空
    created_at INTEGER NOT NULL, -- 创建时间（毫秒时间戳，由生成的insert函数自动写入）
    updated_at INTEGER NOT NULL, -- 更新时间（毫秒时间戳，由生成的update函数自动写入）
    deleted BOOLEAN NOT NULL DEFAULT 0, -- 逻辑删除标记
    deleted_at INTEGER, -- 删除时间（毫秒时间戳）
    deleted_by VARCHAR, -- 删除操作人
    version INTEGER NOT NULL DEFAULT 0 -- 乐观锁版本号
);

-- 标签表：演示简单表（无逻辑删除、无乐观锁），只生成基础CRUD
CREATE TABLE IF NOT EXISTS tag ( -- 标签
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- 主键
    name VARCHAR NOT NULL -- 标签名
);
