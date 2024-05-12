#[cfg(not(debug_assertions))]
use std::panic;
use mlua::prelude::*;
use std::{error::Error, num::NonZeroU32, sync::mpsc, time::SystemTime};
use windows_capture::{
  capture::{CaptureControl, GraphicsCaptureApiHandler},
  frame::Frame,
  graphics_capture_api::InternalCaptureControl,
  monitor::Monitor,
  settings::{ColorFormat, CursorCaptureSettings, DrawBorderSettings, Settings}, window::Window,
};
use fast_image_resize as fr;
mod monitor_or_window;
use monitor_or_window::{find_monitor_or_window, WindowOrMonitor};

struct ImageData {
  data: String,
  width: u32,
  height: u32,
}

struct FlagStruct {
  pub width: Option<i32>,
  pub height: Option<i32>,
  pub frame_rate: Option<i32>,
  pub tx: mpsc::Sender<(String, ImageData)>,
  pub rx: mpsc::Receiver<(String, String)>,
  pub frame_rate_time: f64,
}

struct Capture {
  width: i32,
  height: i32,
  frame_rate: i32,
  tx: mpsc::Sender<(String, ImageData)>,
  rx: mpsc::Receiver<(String, String)>,
  frame_rate_time: f64,
}

impl GraphicsCaptureApiHandler for Capture {
  type Flags = FlagStruct;
  type Error = Box<dyn std::error::Error + Send + Sync>;

  fn new(add_args: Self::Flags) -> Result<Self, Self::Error> {
    Ok(Capture {
      width: add_args.width.unwrap_or(0),
      height: add_args.height.unwrap_or(0),
      frame_rate: add_args.frame_rate.unwrap_or(0),
      tx: add_args.tx,
      rx: add_args.rx,
      frame_rate_time: add_args.frame_rate_time,
    })
  }

  fn on_frame_arrived(
    &mut self,
    frame: &mut Frame,
    capture_control: InternalCaptureControl,
  ) -> Result<(), Self::Error> {
    match self.rx.try_recv() {
      Ok((command, value)) => {
        if command == "setFrameRate" {
          self.frame_rate = value.parse().unwrap_or(0);
        } else if command == "setResolution" {
          let args: Vec<i32> = value.split('@').map(|x| x.parse().unwrap_or(0)).collect();
          
          self.width = args[0];
          self.height = args[1];
        } else if command == "setWidth" {
          self.width = value.parse().unwrap_or(0);
        } else if command == "setHeight" {
          self.height = value.parse().unwrap_or(0);
        } else if command == "stop" {
          self.tx.send(("closed".to_string(), ImageData {
            data: "".to_string(),
            width: 0,
            height: 0,
          })).unwrap();
          capture_control.stop();
        }
      }
      _ => {}
    }

    for (command, value) in self.rx.try_iter() {
      if command == "setFrameRate" {
        self.frame_rate = value.parse().unwrap_or(0);
      } else if command == "setResolution" {
        let args: Vec<i32> = value.split('@').map(|x| x.parse().unwrap_or(0)).collect();
        
        self.width = args[0];
        self.height = args[1];
      } else if command == "setWidth" {
        self.width = value.parse().unwrap_or(0);
      } else if command == "setHeight" {
        self.height = value.parse().unwrap_or(0);
      }
    }

    if self.frame_rate > 0 {
      let cur_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
      let frame_time = 1.0 / self.frame_rate as f64;

      if cur_time - self.frame_rate_time < frame_time {
        return Ok(());
      }

      self.frame_rate_time = cur_time;
    }

    let frame_width = frame.width();
    let frame_height = frame.height();
    let frame_buffer = frame.buffer().unwrap().as_raw_nopadding_buffer().unwrap().to_vec();

    if self.width == 0 || self.height == 0 {
      self.tx.send(("frame".to_string(), ImageData {
        data: unsafe { String::from_utf8_unchecked(frame_buffer) },
        width: frame_width,
        height: frame_height,
      })).unwrap();

      return Ok(());
    } else if self.width == frame_width as i32 && self.height == frame_height as i32 {
      self.tx.send(("frame".to_string(), ImageData {
        data: unsafe { String::from_utf8_unchecked(frame_buffer) },
        width: frame_width,
        height: frame_height,
      })).unwrap();
    } else {
      let src_image = fr::Image::from_vec_u8(
        NonZeroU32::new(frame_width).unwrap(),
        NonZeroU32::new(frame_height).unwrap(),
        frame_buffer,
        fr::PixelType::U8x4,
      ).unwrap();

      let mut dst_image = fr::Image::new(
        NonZeroU32::new(self.width as u32).unwrap(),
        NonZeroU32::new(self.height as u32).unwrap(),
        src_image.pixel_type(),
      );

      let mut dst_view = dst_image.view_mut();

      let mut resizer = fr::Resizer::new(
        fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
      );
      resizer.resize(&src_image.view(), &mut dst_view).unwrap();

      let dst_buffer = dst_image.into_vec();
      
      self.tx.send(("frame".to_string(), ImageData {
        data: unsafe { String::from_utf8_unchecked(dst_buffer) },
        width: self.width as u32,
        height: self.height as u32,
      })).unwrap();
    };

      Ok(())
  }

  fn on_closed(&mut self) -> Result<(), Self::Error> {
    self.tx.send(("closed".to_string(), ImageData {
      data: "".to_string(),
      width: 0,
      height: 0,
    })).unwrap();

    Ok(())
  }
}

struct LuaCapture {
  _capture: CaptureControl<Capture, Box<dyn Error + Sync + Send>>,
  rx: mpsc::Receiver<(String, ImageData)>,
  tx: mpsc::Sender<(String, String)>,
  width: i32,
  height: i32,
  frame_rate: i32,
  frame: ImageData,
  on_close: Option<LuaRegistryKey>,
  running: bool
}

impl LuaUserData for LuaCapture {
  fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
    methods.add_method("setFrameRate", |_, this, frame_rate: i32| {
      this.tx.send(("setFrameRate".to_string(), frame_rate.to_string())).unwrap();

      Ok(())
    });

    methods.add_method("setResolution", |_, this, (width, height): (i32, i32)| {
      this.tx.send(("setResolution".to_string(), format!("{}@{}", width, height))).unwrap();

      Ok(())
    });

    methods.add_method("setWidth", |_, this, width: i32| {
      this.tx.send(("setWidth".to_string(), width.to_string())).unwrap();

      Ok(())
    });

    methods.add_method("setHeight", |_, this, height: i32| {
      this.tx.send(("setHeight".to_string(), height.to_string())).unwrap();

      Ok(())
    });

    methods.add_method("getFrameRate", |_, this, _: ()| {
      Ok(this.frame_rate)
    });

    methods.add_method("getResolution", |_, this, _: ()| {
      Ok((this.width, this.height))
    });

    methods.add_method("getWidth", |_, this, _: ()| {
      Ok(this.width)
    });

    methods.add_method("getHeight", |_, this, _: ()| {
      Ok(this.height)
    });

    methods.add_method_mut("onClose", |lua, this, callback: LuaFunction| {
      this.on_close = Some(lua.create_registry_value(callback)?);

      Ok(())
    });

    methods.add_method_mut("isRunning", |_, this, _: ()| {
      Ok(this.running)
    });

    methods.add_method_mut("stop", |_, this: &mut LuaCapture, _: ()| {
      this.tx.send(("stop".to_string(), "".to_string())).unwrap();
    
      Ok(())
    });

    methods.add_method_mut("updateRender", |lua, this, not_return_data: bool| {
      for (command, value) in this.rx.try_iter() {
        if command == "closed" {
          this.running = false;
          this.frame = ImageData {
            data: "".to_string(),
            width: 0,
            height: 0,
          };
          if let Some(on_close) = &this.on_close {
            lua.registry_value::<LuaFunction>(&on_close).unwrap().call::<_, ()>(()).unwrap();
          }
        } else if command == "frame" {
          if this.running {
            this.frame = value;
          }
        }
      }

      if not_return_data {
        return Ok(("".to_string(), 0, 0));
      }

      Ok((this.frame.data.clone(), this.frame.width, this.frame.height))
    });

    methods.add_method("getFrame", |_, this: &LuaCapture, _: ()| {
      Ok((this.frame.data.clone(), this.frame.width, this.frame.height))
    });
  }
}

fn lua_new(_: &Lua, data: LuaTable) -> LuaResult<LuaCapture> {
  let target = data.get::<_, i32>("target").unwrap_or(0);
  let width = data.get::<_, i32>("width").unwrap_or(0);
  let height = data.get::<_, i32>("height").unwrap_or(0);
  let frame_rate = data.get::<_, i32>("frameRate").unwrap_or(0);

  let capture;

  let (to_lua, from_frame) = mpsc::channel();
  let (to_frame, from_lua) = mpsc::channel();

  let data = FlagStruct {
    width: Some(width),
    height: Some(height),
    frame_rate: Some(frame_rate),
    tx: to_lua,
    rx: from_lua,
    frame_rate_time: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64(),
  };

  match find_monitor_or_window(target) {
    Some(WindowOrMonitor::Monitor(monitor)) => {
      let settings = Settings::new(
        monitor,
        CursorCaptureSettings::Default,
        DrawBorderSettings::WithoutBorder,
        ColorFormat::Rgba8,
        data,
      );

      capture = Capture::start_free_threaded(settings).unwrap();
    }
    Some(WindowOrMonitor::Window(window)) => {
      let settings = Settings::new(
        window,
        CursorCaptureSettings::Default,
        DrawBorderSettings::WithoutBorder,
        ColorFormat::Rgba8,
        data,
      );

      capture = Capture::start_free_threaded(settings).unwrap();
    }
    None => {
      return Err(LuaError::RuntimeError("Invalid target".to_string()));
    }
  }

  Ok(LuaCapture {
    _capture: capture,
    rx: from_frame,
    tx: to_frame,
    width,
    height,
    frame_rate,
    frame: ImageData {
      data: "".to_string(),
      width: 0,
      height: 0,
    },
    on_close: None,
    running: true,
  })
}

fn lua_get_targets(lua: &Lua, _: ()) -> LuaResult<LuaTable> {
  let targets = lua.create_table()?;

  let mut index = 1;

  let monitors = Monitor::enumerate().unwrap();
  for monitor in monitors {
    let target_monitor = lua.create_table()?;
      target_monitor.set("title", monitor.name().unwrap())?;
      target_monitor.set("type", "monitor")?;
      target_monitor.set("id", monitor.index().unwrap() as i32)?;
      target_monitor.set("width", monitor.width().unwrap() as i32)?;
      target_monitor.set("height", monitor.height().unwrap() as i32)?;
      target_monitor.set("refreshRate", monitor.refresh_rate().unwrap() as i32)?;
      target_monitor.set("device", monitor.device_string().unwrap())?;
    targets.set(index, target_monitor)?;

    index += 1;
  }

  let windows = Window::enumerate().unwrap();
  for window in windows {
    let target_window = lua.create_table()?;
      target_window.set("title", window.title().unwrap())?;
      target_window.set("type", "window")?;
      target_window.set("id", window.as_raw_hwnd() as i32)?;
    targets.set(index, target_window)?;

    index += 1;
  }

  Ok(targets)
}

#[mlua::lua_module]
fn screencap(lua: &Lua) -> LuaResult<LuaTable> {
  #[cfg(not(debug_assertions))]
  panic::set_hook(Box::new(|_| {}));

  let exports = lua.create_table()?;
    exports.set("getTargets", lua.create_function(lua_get_targets)?)?;
    exports.set("new", lua.create_function(lua_new)?)?;
    exports.set("version", env!("CARGO_PKG_VERSION"))?;

  Ok(exports)
}
