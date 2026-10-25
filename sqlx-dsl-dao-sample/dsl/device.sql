-- @name update_device
-- @param id:i64 设备id
-- @param weight:f64? 重量
-- @param name:String? 设备名，为空则不更新
UPDATE device SET weight = :weight
-- @if name != null
, name = :name
-- @end
WHERE id = :id

-- ------------------------------

-- 列表为空时 WHERE 1 = 0 不会删除任何数据
-- @name delete_device_by_ids
-- @param ids:List<i64> 设备id列表
DELETE FROM device WHERE 1 = 0
-- @each ids seq="," open="OR id IN (" close=")" item="id"
:id
-- @end

-- ------------------------------

-- @name set_device_offline
-- @param id:i64 设备id
update device set online = 0 where id = :id
