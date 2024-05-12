```lua
local ffi = require("ffi")
local screencap = require("screencap")

function love.load()
  imgData = love.image.newImageData(1, 1)
  img = love.graphics.newImage(imgData)

  -- screencap.version() -- Returns the version of the library

  -- Get all available targets
  local targets = screencap.getTargets()

  -- Create a new capture instance
  capture = screencap.new({
    target = targets[1].id, -- The target's ID
    width = 1920, -- Or 0 for the target's resolution
    height = 1080, -- Or 0 for the target's resolution
    fps = 60, -- Or 0 for no limit
  })

  -- capture:setFrameRate(60)
  -- capture:setResolution(1920, 1080)
  -- capture:setWidth(1920)
  -- capture:setHeight(1080)
  -- capture:getFrameRate()
  -- capture:getResolution()
  -- capture:getWidth()
  -- capture:getHeight()
  -- capture:onClose(function()
  --   print("Capture closed")
  -- end)
  -- capture:isRunning()
  -- capture:getFrame() -- Returns the current frame without updating it
end

function love.draw()
  if capture then
    -- Update the capture set arguments to true to not return the data and only update the frame
    local data, w, h = capture:updateRender()
    if #data > 0 and w > 0 and h > 0 then
      if imgData:getWidth() ~= w or imgData:getHeight() ~= h then
        imgData:release()
        imgData = love.image.newImageData(w, h)
        img:release()
        img = love.graphics.newImage(imgData)
      end

      ffi.copy(ffi.cast("uint32_t*", imgData:getFFIPointer()), data, #data)

      img:replacePixels(imgData)

      local windowWidth, windowHeight = love.graphics.getDimensions()
      local imgWidth, imgHeight = img:getDimensions()
      local scaleX = windowWidth / imgWidth
      local scaleY = windowHeight / imgHeight

      love.graphics.draw(img, 0, 0, 0, scaleX, scaleY)
    end
  end
end
```
