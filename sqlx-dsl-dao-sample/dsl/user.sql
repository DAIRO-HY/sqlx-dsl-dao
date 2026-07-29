-- @name find_user_by_condition
-- @param name:String? 姓名（模糊匹配）
-- @param min_age:i64? 最小年龄
-- @return List<User>
SELECT * FROM user WHERE deleted = 0
-- @if name != null
AND name LIKE '%' || :name || '%'
-- @end
-- @if min_age != null
AND age >= :min_age
-- @end
ORDER BY id DESC

-- ------------------------------

-- @name find_user_by_ids
-- @param ids:List<i64> 用户id列表
-- @return List<User>
SELECT * FROM user WHERE deleted = 0
-- @each ids seq="," open="AND id IN (" close=")" item="id"
:id
-- @end

-- ------------------------------

-- @name count_user
-- @return i64
SELECT count(*) FROM user WHERE deleted = 0

-- ------------------------------

-- @name page_user
-- @page
-- @param name:String? 姓名（模糊匹配）
-- @return User
SELECT * FROM user WHERE deleted = 0
-- @if name != null
AND name LIKE '%' || :name || '%'
-- @end
ORDER BY id DESC

-- ------------------------------

-- @name update_user_profile
-- @param id:i64 用户id
-- @param updated_at:i64 更新时间（毫秒时间戳，由调用方传入）
-- @param name:String? 姓名，为空则不更新
-- @param age:i64? 年龄，为空则不更新
UPDATE user SET updated_at = :updated_at
-- @if name != null
, name = :name
-- @end
-- @if age != null
, age = :age
-- @end
WHERE id = :id AND deleted = 0
