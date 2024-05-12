use windows_capture::{monitor::Monitor, window::Window};

pub enum WindowOrMonitor {
  Window(Window),
  Monitor(Monitor),
}

pub fn find_monitor_or_window(target: i32) -> Option<WindowOrMonitor> {
  let monitors = Monitor::enumerate().unwrap();
  for monitor in monitors {
    if monitor.index().unwrap() == target as usize {
      return Some(WindowOrMonitor::Monitor(monitor));
    }
  }

  let windows = Window::enumerate().unwrap();
  for window in windows {
    if window.as_raw_hwnd() as i32 == target {
      return Some(WindowOrMonitor::Window(window));
    }
  }

  None
}
