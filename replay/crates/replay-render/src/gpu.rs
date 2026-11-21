#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Low,
    Mid,
    High,
}

#[derive(Clone, Debug)]
pub struct Gpu {
    pub name: String,
    pub web_gpu: bool,
    pub tier: Tier,
}

const SOFTWARE: [&str; 6] = [
    "swiftshader",
    "llvmpipe",
    "softpipe",
    "software",
    "basic render",
    "microsoft basic",
];
const FAST: [&str; 10] = [
    "apple",
    "nvidia",
    "geforce",
    "quadro",
    "rtx",
    "gtx",
    "radeon rx",
    "radeon pro",
    "radeon(tm) rx",
    "radeon(tm) pro",
];
const INTEL_ARC: [&str; 4] = ["arc a", "arc b", "arc(tm) a", "arc(tm) b"];

impl Gpu {
    pub(crate) fn new(info: &wgpu::AdapterInfo) -> Gpu {
        let name = info.name.to_lowercase();
        let mentions = |keys: &[&str]| keys.iter().any(|key| name.contains(key));
        let software = info.device_type == wgpu::DeviceType::Cpu
            || mentions(&SOFTWARE)
            || name
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word == "cpu");
        let arc = INTEL_ARC.iter().any(|prefix| {
            name.match_indices(prefix)
                .any(|(at, _)| name[at + prefix.len()..].starts_with(|c: char| c.is_ascii_digit()))
        });
        let tier = if software {
            Tier::Low
        } else if mentions(&FAST) || arc {
            Tier::High
        } else {
            Tier::Mid
        };
        Gpu {
            name: info.name.clone(),
            web_gpu: info.backend == wgpu::Backend::BrowserWebGpu,
            tier,
        }
    }

    pub(crate) fn anisotropy(&self) -> u16 {
        match self.tier {
            Tier::Low => 1,
            Tier::Mid => 4,
            Tier::High => 8,
        }
    }
}
