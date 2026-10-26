//! Windows implementations. Everything here compiles only on Windows.

pub mod basic;
pub mod companion;
pub mod etw;
pub mod gpu;
pub mod hardware;
pub mod overlay;
pub mod pdh;
pub mod proc;
pub mod screenshot;
pub mod system;

use crate::provider::SensorProvider;
use fpsframer_core::model::Companions;

/// Priority order: built-in sources first, companion tools only fill the gaps.
pub fn default_providers(companions: Companions, primary_gpu: Option<usize>) -> Vec<Box<dyn SensorProvider>> {
    let mut v: Vec<Box<dyn SensorProvider>> = vec![Box::new(basic::WindowsBasic::new()), Box::new(gpu::WindowsGpu::new(primary_gpu))];
    if companions.hwinfo {
        v.push(Box::new(companion::HwinfoProvider::new()));
    }
    if companions.libre_hardware_monitor {
        v.push(Box::new(companion::LhmProvider::new()));
    }
    v
}
