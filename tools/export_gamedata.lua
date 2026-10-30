-- Execute Rovio's converted 64-bit Lua 5.1 chunks and export clean JSON (v1.2.0)
local dir = "/var/tmp/ab120_luac64/"
local lvldir = dir .. "levels/"

dofile(dir .. "scripts/blocks.lua")
local atlas = dofile("/home/user/angry_birds_120_rust/tools/atlas_sprites.lua")
dofile(dir .. "scripts/starLimits.lua")
dofile(dir .. "scripts/animations.lua")
dofile(dir .. "scripts/particles.lua")

local pack1 = {
  "Level1","Level57","Level53","Level3","Level6","Level2","Level4","Level5","Level7","Level8","Level9","Level13","Level10","Level39","Level12","Level15","Level17","Level14","Level16","Level23","Level44",
  "Level52","Level34","Level42","Level24","Level88","Level36","Level31","Level21","Level41","Level76","Level38","Level35","Level20","Level26","Level66","Level85","Level27","Level32","Level72","Level90","Level96",
  "Level43","Level77","Level28","Level29","Level87","Level18","Level91","Level49","Level45","Level75","Level51","Level30","Level79","Level40","Level59","Level58","Level95","Level82","Level22","Level89","Level81"
}
local pack2 = {
  "LevelP2_103","LevelP2_91","LevelP2_65","LevelP2_96","LevelP2_69","LevelP2_88","LevelP2_64","LevelP2_80","LevelP2_108","LevelP2_85","LevelP2_82","LevelP2_66","LevelP2_104","LevelP2_210","LevelP2_83","LevelP2_79","LevelP2_77","LevelP2_114","LevelP2_81","LevelP2_68","LevelP2_95",
  "LevelP2_78","LevelP2_100","LevelP2_92","LevelP2_94","LevelP2_89","LevelP2_73","LevelP2_76","LevelP2_122","LevelP2_99","LevelP2_84","LevelP2_86","LevelP2_74","LevelP2_115","LevelP2_98","LevelP2_71","LevelP2_72","LevelP2_87","LevelP2_93","LevelP2_67","LevelP2_97","LevelP2_90"
}
local level_order = {}
for _,n in ipairs(pack1) do level_order[#level_order+1] = {name=n, pack=1} end
for _,n in ipairs(pack2) do level_order[#level_order+1] = {name=n, pack=4} end

local star_limits = {}
for _, e in ipairs(level_order) do
  local st = _G[e.name]
  local rec = { silver = 20000, gold = 30000 }
  if type(st) == "table" then
    rec.silver = st.silverScore or st.silver or st[1] or 20000
    rec.gold = st.goldScore or st.gold or st[2] or 30000
  end
  star_limits[e.name] = rec
end

local saved_blocks = blocks
local saved_materials = materials
local saved_themes = themes
local saved_particles = particles

local function find_file(name)
  for _, p in ipairs({"pack1","pack2","pack3","pack4","pack5"}) do
    local f = io.open(lvldir .. p .. "/" .. name .. ".lua", "rb")
    if f then f:close(); return lvldir .. p .. "/" .. name .. ".lua" end
  end
  return nil
end


-- ---------------------------------------------------------------------------------------
-- Block size convention differs between chapters.
-- Chapters 1-3 (pack1..pack3) store `width`/`height` as Box2D *half extents* (the value
-- times 2*physicsToWorld equals the sprite's pixel width). Every chapter 4-5 stage
-- (`pack4/`, `pack5/`, `LevelP2_*`) was authored with *full* extents instead, so using
-- them directly made every collider twice the size of its artwork. Packed castles then
-- interpenetrated and blew themselves apart the moment the level loaded (49 of 105 levels).
-- We detect the convention per level from the artwork and normalise to half extents.
local function sprite_dim(name, ptw)
  local a = atlas[name]
  if not a or not a.w then return nil, nil end
  return a.w / ptw, a.h / ptw
end

local function detect_half_extents(world_tbl, ptw)
  local ratios = {}
  for _, o in pairs(world_tbl or {}) do
    if (o.type or "box") == "box" and (o.width or 0) > 0 and (o.height or 0) > 0 then
      local def = saved_blocks[o.definition or ""] or {}
      local spr = o.sprite
      if not (spr and atlas[spr]) then spr = def.sprite end
      local sw, _sh = sprite_dim(spr, ptw)
      if sw then
        ratios[#ratios + 1] = sw / (o.width * 2.0)
      end
    end
  end
  if #ratios < 3 then return true end
  table.sort(ratios)
  local median = ratios[math.floor((#ratios + 1) / 2)]
  -- ~1.0 => the stored width already is a half extent; ~0.54 => it is a full extent.
  return median > 0.75
end

local levels_out = {}
for idx, e in ipairs(level_order) do
  world = nil
  joints = nil
  theme = nil
  castleCameraData = nil
  birdCameraData = nil
  physicsToWorld = 20
  local fp = find_file(e.name)
  if not fp then print("MISSING LEVEL " .. e.name) end
  dofile(fp)
  local half_extents = detect_half_extents(world, physicsToWorld)
  if not half_extents then
    for _, o in pairs(world or {}) do
      if (o.width or 0) > 0 then o.width = o.width * 0.5 end
      if (o.height or 0) > 0 then o.height = o.height * 0.5 end
      if (o.radius or 0) > 0 then o.radius = o.radius * 0.5 end
    end
  end
  local objs = {}
  for oname, o in pairs(world or {}) do
    if oname ~= "ground" then
      local def = saved_blocks[o.definition or ""] or {}
      -- Resolve sprites against the real 1.2.0 atlas: several levels still carry the old
      -- editor sprite ids (e.g. GROUND_BLOCK_THEME01_04) that no longer exist in the atlas,
      -- which used to leave whole cliffs / static terrain invisible.
      local function pick(...)
        for i = 1, select("#", ...) do
          local n = select(i, ...)
          if type(n) == "string" and n ~= "" and atlas[n] then return n end
        end
        return ""
      end
      local def_dmg = ""
      for _, t in pairs(def.damageSprites or {}) do
        if type(t) == "table" and type(t.sprite) == "string" then def_dmg = t.sprite break end
      end
      local spr = pick(o.sprite, def.sprite, def_dmg)
      local dmg = pick(o.damageSprite, def.damageSprite, def.sprite2, o.sprite, def.sprite)
      local blk = pick(o.blinkSprite, def.blinkSprite)
      local smi = pick(o.smileSprite, def.smileSprite)
      local px = o.spritePivotX or def.spritePivotX
      local py = o.spritePivotY or def.spritePivotY
      objs[#objs + 1] = {
        name = o.name or oname,
        definition = o.definition or "",
        sprite = spr,
        damageSprite = dmg,
        blinkSprite = blk,
        smileSprite = smi,
        shape_type = o.type or def.type or "box",
        x = o.x or 0,
        y = o.y or 0,
        angle = o.angle or 0,
        width = o.width or 0,
        height = o.height or 0,
        radius = o.radius or 0,
        density = o.density or def.density,
        friction = o.friction or def.friction,
        restitution = o.restitution or def.restitution,
        strength = o.strength or def.strength,
        defence = o.defence or def.defence,
        controllable = o.controllable or false,
        levelGoal = o.levelGoal or false,
        startNumber = o.startNumber or 0,
        material = o.material or def.material or "",
        spritePivotX = px,
        spritePivotY = py,
      }
    end
  end
  table.sort(objs, function(a, b) return a.name < b.name end)

  local jlist = {}
  for jname, j in pairs(joints or {}) do
    jlist[#jlist + 1] = {
      name = j.name or jname,
      joint_type = j.type or 1,
      end1 = j.end1 or "",
      end2 = j.end2 or "",
      x1 = j.x1 or 0,
      y1 = j.y1 or 0,
      x2 = j.x2 or 0,
      y2 = j.y2 or 0,
      coordType = j.coordType or 0,
    }
  end
  table.sort(jlist, function(a, b) return a.name < b.name end)

  levels_out[#levels_out + 1] = {
    index = idx,
    chapter = math.floor((idx - 1) / 21) + 1,
    stage = ((idx - 1) % 21) + 1,
    id = e.name,
    theme = theme or "theme1",
    physicsToWorld = physicsToWorld or 20,
    halfExtents = half_extents,
    stars = star_limits[e.name] or {},
    birdCamera = birdCameraData or {},
    castleCamera = castleCameraData or {},
    objects = objs,
    joints = jlist,
  }
end

local function to_json(v)
  local t = type(v)
  if t == "nil" then return "null"
  elseif t == "boolean" then return v and "true" or "false"
  elseif t == "number" then
    if v ~= v or v == math.huge or v == -math.huge then return "0" end
    return string.format("%.6g", v)
  elseif t == "string" then
    local esc = v:gsub("\\", "\\\\"):gsub("\"", "\\\""):gsub("\n", "\\n"):gsub("\r", "\\r"):gsub("\t", "\\t")
    return "\"" .. esc .. "\""
  elseif t == "table" then
    local n = #v
    local is_arr = true
    local cnt = 0
    for k,_ in pairs(v) do
      cnt = cnt + 1
      if type(k) ~= "number" or k < 1 or k > n or math.floor(k) ~= k then
        is_arr = false
        break
      end
    end
    if cnt ~= n then is_arr = false end
    if is_arr and cnt > 0 then
      local parts = {}
      for i=1,n do parts[i] = to_json(v[i]) end
      return "[" .. table.concat(parts, ",") .. "]"
    elseif cnt == 0 then
      return "[]"
    else
      local keys = {}
      for k,_ in pairs(v) do keys[#keys+1] = k end
      table.sort(keys, function(a,b) return tostring(a) < tostring(b) end)
      local parts = {}
      for i,k in ipairs(keys) do
        parts[i] = to_json(tostring(k)) .. ":" .. to_json(v[k])
      end
      return "{" .. table.concat(parts, ",") .. "}"
    end
  end
  return "null"
end

local root = {
  materials = saved_materials,
  themes = saved_themes,
  blocks = saved_blocks,
  levels = levels_out,
}

local f = io.open("/home/user/angry_birds_120_rust/assets/gamedata.json", "wb")
f:write(to_json(root))
f:close()
print("Wrote gamedata.json with " .. #levels_out .. " levels!")
