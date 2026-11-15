#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use eframe::egui::{self, Color32, RichText, Stroke, Vec2, ViewportCommand};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, process::Command, time::{Duration, Instant}};

#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;

fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") { "Windows" } else if cfg!(target_os = "macos") { "macOS" } else { "Linux" }
}
fn settings_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    return std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("AULA L99/settings.json"));
    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support/AULA L99/settings.json"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config/aula-l99-linux/settings.json"))
}

#[derive(Default, Clone)]
struct LiveMetrics { cpu_load: u8, cpu_temp: Option<u8>, gpu_load: Option<u8>, gpu_temp: Option<u8> }
fn cpu_counters() -> Option<(u64, u64)> {
    let line = fs::read_to_string("/proc/stat").ok()?.lines().next()?.to_owned();
    let values = line.split_whitespace().skip(1).filter_map(|v| v.parse::<u64>().ok()).collect::<Vec<_>>();
    if values.len() < 4 { return None; }
    Some((values.iter().sum(), values[3] + values.get(4).copied().unwrap_or(0)))
}
#[derive(Clone)]
struct TemperatureSensor { id: String, label: String, value: u8, cpu: bool, priority: u8 }
fn temperature_sensors(root: &std::path::Path) -> Vec<TemperatureSensor> {
    let mut sensors=Vec::new();
    if let Ok(dirs)=fs::read_dir(root) {for dir in dirs.flatten() {
        let path=dir.path();
        let name=fs::read_to_string(path.join("name")).unwrap_or_default().trim().to_lowercase();
        let cpu=["cpu","coretemp","k10temp","zenpower","k8temp","fam15h","peci"].iter().any(|s|name.contains(s));
        if let Ok(inputs)=fs::read_dir(&path) {for input in inputs.flatten() {
            let file=input.file_name().to_string_lossy().to_string();
            if !file.starts_with("temp")||!file.ends_with("_input"){continue;}
            let Some(value)=fs::read_to_string(input.path()).ok().and_then(|s|s.trim().parse::<i64>().ok()).filter(|v|(0..=255000).contains(v)).map(|v|(v/1000) as u8) else {continue;};
            let label=fs::read_to_string(path.join(file.replace("_input","_label"))).unwrap_or_else(|_|file.clone()).trim().to_owned();
            let priority=if label.contains("Tdie")||label.contains("Package") {0}else if label.contains("Tctl"){1}else{2};
            sensors.push(TemperatureSensor{id:format!("{name}:{file}"),label:format!("{name} · {label} · {value}°C"),value,cpu,priority});
        }}
    }}
    sensors.sort_by(|a,b|(a.priority,&a.id).cmp(&(b.priority,&b.id)));
    sensors
}
fn selected_cpu_temperature(selected: &str) -> Option<u8> {
    let sensors=temperature_sensors(std::path::Path::new("/sys/class/hwmon"));
    if !selected.is_empty() {return sensors.iter().find(|s|s.id==selected).map(|s|s.value);}
    if let Some(sensor)=sensors.iter().find(|s|s.cpu){return Some(sensor.value);}
    for dir in fs::read_dir("/sys/class/thermal").ok()?.flatten() {
        let kind=fs::read_to_string(dir.path().join("type")).unwrap_or_default().to_lowercase();
        if kind.contains("cpu")||kind.contains("x86_pkg") {
            if let Some(value)=fs::read_to_string(dir.path().join("temp")).ok().and_then(|s|s.trim().parse::<i64>().ok()).filter(|v|(0..=255000).contains(v)) {return Some((value/1000) as u8);}
        }
    }
    None
}
fn gpu_metrics() -> (Option<u8>,Option<u8>) {
    if let Ok(output)=Command::new("nvidia-smi").args(["--query-gpu=utilization.gpu,temperature.gpu","--format=csv,noheader,nounits"]).output(){
        if output.status.success(){let text=String::from_utf8_lossy(&output.stdout);let mut values=text.lines().next().unwrap_or("").split(',').map(|v|v.trim().parse::<u8>().ok());let load=values.next().flatten();let temp=values.next().flatten();if load.is_some()||temp.is_some(){return(load,temp);}}
    }
    if let Ok(cards)=fs::read_dir("/sys/class/drm") {for card in cards.flatten(){
        let path=card.path().join("device");
        let load=fs::read_to_string(path.join("gpu_busy_percent")).ok().and_then(|s|s.trim().parse::<u8>().ok()).map(|v|v.min(100));
        let mut temp=None;
        if let Ok(sensors)=fs::read_dir(path.join("hwmon")){for sensor in sensors.flatten(){temp=fs::read_to_string(sensor.path().join("temp1_input")).ok().and_then(|s|s.trim().parse::<i64>().ok()).map(|v|(v/1000).clamp(0,255) as u8);if temp.is_some(){break;}}}
        if load.is_some()||temp.is_some(){return(load,temp);}
    }}
    (None,None)
}
fn rtc_block(metrics: &LiveMetrics, cpu_thermometer: bool) -> Option<[u8; 64]> {
    let output = Command::new("date").args(["+%y %m %d %H %M %S %w"]).output().ok()?;
    let fields = String::from_utf8(output.stdout).ok()?.split_whitespace().filter_map(|s| s.parse::<u8>().ok()).collect::<Vec<_>>();
    if fields.len() != 7 { return None; }
    let mut b = [0u8; 64];
    b[1] = 1; b[2] = 0x5a;
    b[3..9].copy_from_slice(&fields[..6]); b[10] = fields[6];
    b[13] = metrics.cpu_load; b[14] = metrics.cpu_temp.unwrap_or(0);
    b[15] = metrics.gpu_load.unwrap_or(0); b[16] = metrics.gpu_temp.unwrap_or(0);
    // The LCD's small thermometer is the weather/air field, not byte 14.
    // Explicit CPU mode mirrors Celsius there; humidity/weather stay unset.
    if cpu_thermometer { b[17] = metrics.cpu_temp.unwrap_or(0); }
    b[62] = 0xaa; b[63] = 0x55;
    Some(b)
}
fn screen_clock() -> String {
    Command::new("date").args(["+%H:%M"]).output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok()).map(|s| s.trim().to_owned()).unwrap_or_else(|| "--:--".into())
}
#[cfg(target_os = "linux")]
unsafe extern "C" { fn ioctl(fd: i32, request: u64, ...) -> i32; }
#[cfg(target_os = "linux")]
fn hid_feature(fd: i32, packet: &[u8; 64], read_reply: bool) -> Result<[u8; 65], String> {
    let mut report = [0u8; 65];
    report[1..].copy_from_slice(packet);
    let request = if read_reply { 0xC041_4807u64 } else { 0xC041_4806u64 };
    if unsafe { ioctl(fd, request, report.as_mut_ptr()) } < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(report)
}
#[cfg(target_os = "linux")]
fn realtime_hid_path() -> Option<PathBuf> {
    for entry in fs::read_dir("/sys/class/hidraw").ok()?.flatten() {
        let Ok(device) = fs::canonicalize(entry.path().join("device")) else { continue; };
        let mut cursor = device;
        let mut vendor = String::new();
        let mut product = String::new();
        let mut interface = String::new();
        loop {
            if vendor.is_empty() { vendor = fs::read_to_string(cursor.join("idVendor")).unwrap_or_default().trim().to_lowercase(); }
            if product.is_empty() { product = fs::read_to_string(cursor.join("idProduct")).unwrap_or_default().trim().to_lowercase(); }
            if interface.is_empty() { interface = fs::read_to_string(cursor.join("bInterfaceNumber")).unwrap_or_default().trim().to_lowercase(); }
            if let Some(parent) = cursor.parent() { cursor = parent.to_path_buf(); } else { break; }
        }
        if vendor == "0c45" && product == "800a" && interface == "03" {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}
#[cfg(target_os = "linux")]
fn open_hid() -> Result<fs::File, String> {
    let path = realtime_hid_path().ok_or_else(|| "Wired L99 keyboard (0C45:800A, HID interface 3) not found. Connect it by USB and rescan.".to_string())?;
    fs::OpenOptions::new().read(true).write(true).open(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            format!("Permission denied opening {}. Run sh app/install-permissions.sh from the project folder to repair Linux access, then retry. See Device → Connection notes.", path.display())
        } else {
            format!("Cannot open {}: {error}", path.display())
        }
    })
}
#[cfg(target_os = "linux")]
fn hid_command(fd: i32, opcode: u8, blocks: u8, byte2: u8) -> Result<(), String> {
    // The protocol uses ACK_FLAG (bit 0), not an exact status byte.
    // Reference: Aula_L99_Linux/tools/aula_l99_hacky/cli.py::_acked.

    let mut packet = [0u8; 64];
    packet[0] = 4;
    packet[1] = opcode;
    packet[2] = byte2;
    packet[8] = blocks;
    hid_feature(fd, &packet, false)?;
    std::thread::sleep(Duration::from_millis(10));
    let reply = hid_feature(fd, &packet, true)?;
    // report[0] is the Linux hidraw report ID; protocol bytes start at 1.
    // The reply is 04 <opcode> 00 <ack>, so the opcode is report[2].
    if !command_acknowledged(&reply, opcode) {
        return Err(format!("Keyboard did not acknowledge command 0x{opcode:02x} (reply {:02x} {:02x} {:02x} {:02x})", reply[1], reply[2], reply[3], reply[4]));
    }
    Ok(())
}
fn command_acknowledged(reply: &[u8; 65], opcode: u8) -> bool {
    reply[1] == 4 && reply[2] == opcode && (reply[4] & 1) != 0
}

#[cfg(target_os = "linux")]
fn hid_transfer(fd: i32, opcode: u8, blocks: &[[u8; 64]], byte2: u8) -> Result<(), String> {
    hid_command(fd, 0x18, 0, 0)?;
    hid_command(fd, opcode, blocks.len() as u8, byte2)?;
    for block in blocks {
        hid_feature(fd, block, false)?;
        std::thread::sleep(Duration::from_millis(10));
    }
    hid_command(fd, 0x02, 0, 0)?;
    hid_command(fd, 0xf0, 0, 0)?;
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn send_live_rtc(_: &LiveMetrics, _: bool) -> Result<(), String> { Err("Live hardware monitoring is currently supported on Linux only".into()) }
#[cfg(not(target_os = "linux"))]
fn send_keyboard_color(_: [u8; 3]) -> Result<(), String> { Err("USB lighting transport has not been ported to this platform".into()) }
#[cfg(not(target_os = "linux"))]
fn send_lighting(_: &Settings) -> Result<(), String> { Err("USB lighting transport has not been ported to this platform".into()) }
#[cfg(not(target_os = "linux"))]
fn send_keyboard_settings(_: &Settings) -> Result<(), String> { Err("USB settings transport has not been ported to this platform".into()) }
const L99_KEY_IDS: [u8; 84] = [1,2,3,4,5,6,7,8,9,10,11,12,13,119,112,113,115,19,20,21,22,23,24,25,26,27,28,29,30,31,103,117,37,38,39,40,41,42,43,44,45,46,47,48,49,67,118,55,56,57,58,59,60,61,62,63,64,65,66,85,121,73,74,75,76,77,78,79,80,81,82,83,84,101,120,91,92,93,94,96,98,99,100,102];
const KEY_RECTS: [[f32;4];84] = [
[32.0,34.0,30.0,30.0],
[97.0,34.0,30.0,30.0],
[135.0,34.0,30.0,30.0],
[174.0,34.0,30.0,30.0],
[212.0,34.0,30.0,30.0],
[268.0,34.0,30.0,30.0],
[306.0,34.0,30.0,30.0],
[345.0,34.0,30.0,30.0],
[383.0,34.0,30.0,30.0],
[440.0,34.0,30.0,30.0],
[478.0,34.0,30.0,30.0],
[516.0,34.0,30.0,30.0],
[555.0,34.0,30.0,30.0],
[605.0,34.0,30.0,30.0],
[655.0,34.0,30.0,30.0],
[695.0,34.0,30.0,30.0],
[735.0,34.0,30.0,30.0],
[32.0,83.0,30.0,30.0],
[69.0,83.0,30.0,30.0],
[107.0,83.0,30.0,30.0],
[145.0,83.0,30.0,30.0],
[184.0,83.0,30.0,30.0],
[222.0,83.0,30.0,30.0],
[261.0,83.0,30.0,30.0],
[299.0,83.0,30.0,30.0],
[337.0,83.0,30.0,30.0],
[375.0,83.0,30.0,30.0],
[414.0,83.0,30.0,30.0],
[452.0,83.0,30.0,30.0],
[490.0,83.0,30.0,30.0],
[529.0,83.0,67.0,30.0],
[605.0,83.0,30.0,30.0],
[32.0,121.0,49.0,30.0],
[89.0,121.0,30.0,30.0],
[127.0,121.0,30.0,30.0],
[165.0,121.0,30.0,30.0],
[203.0,121.0,30.0,30.0],
[241.0,121.0,30.0,30.0],
[279.0,121.0,30.0,30.0],
[318.0,121.0,30.0,30.0],
[356.0,121.0,30.0,30.0],
[394.0,121.0,30.0,30.0],
[432.0,121.0,30.0,30.0],
[471.0,121.0,30.0,30.0],
[509.0,121.0,30.0,30.0],
[547.0,121.0,49.0,30.0],
[605.0,121.0,30.0,30.0],
[32.0,157.0,58.0,30.0],
[98.0,157.0,30.0,30.0],
[136.0,157.0,30.0,30.0],
[175.0,157.0,30.0,30.0],
[213.0,157.0,30.0,30.0],
[251.0,157.0,30.0,30.0],
[289.0,157.0,30.0,30.0],
[327.0,157.0,30.0,30.0],
[366.0,157.0,30.0,30.0],
[404.0,157.0,30.0,30.0],
[442.0,157.0,30.0,30.0],
[480.0,157.0,30.0,30.0],
[519.0,157.0,77.0,30.0],
[605.0,157.0,30.0,30.0],
[32.0,196.0,77.0,30.0],
[117.0,196.0,30.0,30.0],
[156.0,196.0,30.0,30.0],
[194.0,196.0,30.0,30.0],
[232.0,196.0,30.0,30.0],
[270.0,196.0,30.0,30.0],
[308.0,196.0,30.0,30.0],
[346.0,196.0,30.0,30.0],
[385.0,196.0,30.0,30.0],
[423.0,196.0,30.0,30.0],
[461.0,196.0,30.0,30.0],
[499.0,196.0,59.0,30.0],
[566.0,196.0,30.0,30.0],
[605.0,196.0,30.0,30.0],
[32.0,235.0,39.0,30.0],
[80.0,235.0,39.0,30.0],
[127.0,235.0,39.0,30.0],
[175.0,235.0,230.0,30.0],
[414.0,235.0,39.0,30.0],
[462.0,235.0,39.0,30.0],
[528.0,235.0,30.0,30.0],
[566.0,235.0,30.0,30.0],
[604.0,235.0,30.0,30.0]];
#[cfg(target_os = "linux")]
fn send_live_rtc(metrics: &LiveMetrics, cpu_thermometer: bool) -> Result<(), String> {
    let file = open_hid()?;
    let fd = file.as_raw_fd();
    let block = rtc_block(metrics, cpu_thermometer).ok_or_else(|| "Could not read the system clock".to_string())?;
    hid_transfer(fd, 0x28, &[block], 0)
}
#[cfg(target_os = "linux")]
fn send_keyboard_color(rgb: [u8; 3]) -> Result<(), String> {
    let file = open_hid()?;
    let fd = file.as_raw_fd();
    let mut blocks = [[0u8; 64]; 9];
    for key_id in L99_KEY_IDS {
        let row = (key_id >> 4) as usize;
        let column = (key_id & 0x0f) as usize;
        let offset = column * 4;
        blocks[row][offset] = key_id;
        blocks[row][offset + 1..offset + 4].copy_from_slice(&rgb);
    }
    blocks[8][62] = 0xaa;
    blocks[8][63] = 0x55;
    hid_transfer(fd, 0x23, &blocks, 0)?;
    let mut effect = [0u8; 64];
    effect[0] = 1;
    effect[1..4].copy_from_slice(&rgb);
    effect[8] = 1;
    effect[9] = 5;
    effect[10] = 3;
    effect[14] = 0xaa;
    effect[15] = 0x55;
    hid_transfer(fd, 0x13, &[effect], 0)
}
#[cfg(target_os = "linux")]
fn send_lighting(settings: &Settings) -> Result<(), String> {
    let file = open_hid()?;
    let fd = file.as_raw_fd();
    let mut effect = [0u8; 64];
    effect[0] = if settings.rgb_spectrum && settings.effect != 19 { 8 } else { (settings.effect + 1).min(20) as u8 };
    effect[1..4].copy_from_slice(&settings.color[..3]);
    effect[8] = if matches!(effect[0], 4 | 7) { 0 } else { 1 };
    effect[9] = (settings.brightness as u16 * 5 / 100).clamp(1, 5) as u8;
    effect[10] = (settings.speed as u16 * 4 / 100 + 1).clamp(1, 5) as u8;
    effect[14] = 0xaa;
    effect[15] = 0x55;
    hid_transfer(fd, 0x13, &[effect], 0)
}
#[cfg(target_os = "linux")]
fn send_keyboard_settings(settings: &Settings) -> Result<(), String> {
    let file = open_hid()?;
    let fd = file.as_raw_fd();
    let mut block = [0u8; 64];
    block[1] = 1;
    block[6] = settings.sleep.min(3) as u8;
    block[8] = (settings.response + 1).clamp(1, 5) as u8;
    block[62] = 0xaa;
    block[63] = 0x55;
    hid_transfer(fd, 0x17, &[block], 1)
}

const EFFECTS: [&str; 20] = [
    "Static", "Breathing", "Color cycle", "Rainbow wave", "Wave", "Ripple", "Reactive",
    "Stars", "Aurora", "Neon", "Snake", "Comet", "Waterfall", "Spectrum", "Scan",
    "Marquee", "Fire", "Cloud", "Cross", "Off",
];
const NAV: [&str; 10] = ["Device", "Keyboard", "Lighting", "Key Mapping", "Touchscreen", "Music", "Config", "Firmware Update", "LCD guide", "Software & manuals"];
// AULA's L99 is an 84-key compact layout. Keep the physical rows and the
// separated navigation cluster in the same order as the product layout.
const KEYS: [&str; 84] = [
    "Esc","F1","F2","F3","F4","F5","F6","F7","F8","F9","F10","F11","F12","Del","PrtSc","ScrLk","Pause",
    "~","1","2","3","4","5","6","7","8","9","0","-","=","Back","Home",
    "Tab","Q","W","E","R","T","Y","U","I","O","P","[","]","\\","PgUp",
    "Caps","A","S","D","F","G","H","J","K","L",";","'","Enter","PgDn",
    "Shift","Z","X","C","V","B","N","M",",",".","/","Shift","↑","End",
    "Ctrl","Win","Alt","Space","Fn","Ctrl","←","↓","→",
];

// Linux evdev codes in the same physical order as KEYS.
const KEY_CODES: [u16;84] = [1,59,60,61,62,63,64,65,66,67,68,87,88,111,99,70,119,41,2,3,4,5,6,7,8,9,10,11,12,13,14,102,15,16,17,18,19,20,21,22,23,24,25,26,27,43,104,58,30,31,32,33,34,35,36,37,38,39,40,28,109,42,44,45,46,47,48,49,50,51,52,53,54,103,107,29,125,56,57,464,97,105,108,106];
#[derive(Serialize, Deserialize, Clone)]
struct KeyMapping { key: usize, target: usize }
fn mapping_payload(settings: &Settings) -> String {
    let entries: Vec<(u16,Vec<u16>)> = settings.mappings.iter().filter_map(|m| {
        let source = *KEY_CODES.get(m.key)?;
        if source == 464 { return None; }
        let output = match m.target {0=>vec![100],1=>if settings.at_shift2 {vec![42,3]} else {vec![100,16]},2=>vec![],n=>vec![*KEY_CODES.get(n-3)?]};
        Some((source,output))
    }).collect();
    if settings.ctrl_alt_altgr || settings.shift2_at {
        serde_json::json!({"mappings": entries, "ctrl_alt_altgr": settings.ctrl_alt_altgr, "shift2_at": settings.shift2_at}).to_string()
    } else { serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into()) }
}
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    cpu_thermometer: bool, mappings: Vec<KeyMapping>, ctrl_alt_altgr: bool, shift2_at: bool, at_shift2: bool, cpu_sensor: String,
    effect: usize, speed: u8, brightness: u8, color: [u8; 4], response: usize,
    sleep: usize, selected_key: usize, weather: String, image_path: String,
    rgb_spectrum: bool, screen_brightness: u8, screen_view: usize, upload_target: usize,
}
impl Default for Settings {
    fn default() -> Self { Self { cpu_thermometer: true, mappings: Vec::new(), ctrl_alt_altgr: false, shift2_at: false, at_shift2: false, cpu_sensor: String::new(), effect: 3, speed: 50, brightness: 75, color: [55, 190, 255, 255], response: 2, sleep: 0, selected_key: 40, weather: String::new(), image_path: String::new(), rgb_spectrum: false, screen_brightness: 80, screen_view: 0, upload_target: 0 } }
}

impl Settings {
    fn validate(&mut self) {
        self.effect = self.effect.min(EFFECTS.len() - 1);
        self.response = self.response.min(4);
        self.sleep = self.sleep.min(3);
        self.selected_key = self.selected_key.min(KEYS.len() - 1);
        self.upload_target = self.upload_target.min(2);
        self.screen_view = self.screen_view.min(2);
        self.speed = self.speed.min(100);
        self.brightness = self.brightness.min(100);
        self.screen_brightness = self.screen_brightness.min(100);
        self.mappings.retain(|m| m.key < KEYS.len() && m.key != 79 && m.target < KEYS.len() + 3 && m.target != 82);
        self.mappings.truncate(KEYS.len());
    }
}

struct DriverApp {
    mapping_child: Option<std::process::Child>,
    mapping_messages: Option<std::sync::mpsc::Receiver<String>>,
    page: usize,
    settings: Settings,
    notice: String,
    screenshot: Option<PathBuf>,
    frame_count: usize,
    capture_requested: bool,
    started: Instant,
    connected: bool,
    device_status: String,
    screen_connected: bool,
    screen_status: String,
    last_device_scan: Instant,
    realtime_enabled: bool,
    realtime_status: String,
    live_metrics: LiveMetrics,
    previous_cpu: Option<(u64, u64)>,
    last_metrics_update: Instant,
    upload: Option<std::sync::mpsc::Receiver<String>>,
    preview: Option<egui::TextureHandle>,
}
impl DriverApp {
    fn new(screenshot: Option<PathBuf>) -> Self {
        let mut app = Self { mapping_child: None, mapping_messages: None, page: 3, settings: Settings::default(), notice: "Scanning for the AULA L99 keyboard".into(), screenshot, frame_count: 0, capture_requested: false, started: Instant::now(), connected: false, device_status: "Checking USB devices…".into(), screen_connected: false, screen_status: "Checking USB serial ports…".into(), last_device_scan: Instant::now() - Duration::from_secs(2), realtime_enabled: false, realtime_status: "Live data is off".into(), live_metrics: LiveMetrics::default(), previous_cpu: cpu_counters(), last_metrics_update: Instant::now() - Duration::from_secs(1), upload: None, preview: None };
        app.scan_keyboard();
        if let Some(p) = settings_path() {
            if let Ok(file) = fs::File::open(p) {
                use std::io::Read;
                let mut bytes = Vec::new();
                if file.take(65537).read_to_end(&mut bytes).is_ok() && bytes.len() <= 65536 {
                    if let Ok(mut settings) = serde_json::from_slice::<Settings>(&bytes) {
                        if settings.ctrl_alt_altgr { settings.ctrl_alt_altgr = false; settings.shift2_at = true; }
                        settings.validate();
                        app.settings = settings;
                    }
                }
            }
        }
        if std::env::args().any(|a|a=="--mapping") {app.page=3;}
        if std::env::args().any(|a|a=="--touchscreen") {app.page=4;}
        if std::env::args().any(|a|a=="--software") {app.page=9;}
        app
    }
    fn scan_keyboard(&mut self) {
        if !cfg!(target_os = "linux") {
            self.last_device_scan = Instant::now();
            self.connected = false;
            self.screen_connected = false;
            self.device_status = "USB device access is not yet ported to this platform".into();
            self.screen_status = "LCD uploads are currently available on Linux only".into();
            self.notice = format!("{} desktop preview · hardware controls require Linux", platform_name());
            return;
        }
        // The keyboard and display are separate USB functions with different IDs.
        // Scan both so a connected keyboard does not mask a missing panel.
        let mut keyboards = Vec::new();
        let mut panel_usb = false;
        if let Ok(devices) = fs::read_dir("/sys/bus/usb/devices") {
            for entry in devices.flatten() {
                let path = entry.path();
                let vendor = fs::read_to_string(path.join("idVendor")).unwrap_or_default().trim().to_ascii_lowercase();
                let product = fs::read_to_string(path.join("idProduct")).unwrap_or_default().trim().to_ascii_lowercase();
                if (vendor == "0c45" && product == "800a") || (vendor == "05ac" && product == "024f") {
                    keyboards.push(format!("{}:{}", vendor.to_ascii_uppercase(), product.to_ascii_uppercase()));
                }
                if vendor == "eeef" && product == "268a" { panel_usb = true; }
            }
        }
        let serial_ports = fs::read_dir("/dev")
            .map(|ports| ports.flatten().filter_map(|p| p.file_name().to_str().map(str::to_owned))
                .filter(|name| name.starts_with("ttyACM")).collect::<Vec<_>>())
            .unwrap_or_default();
        self.last_device_scan = Instant::now();
        self.connected = !keyboards.is_empty();
        self.device_status = if self.connected {
            format!("USB HID detected · {}", keyboards.join(", "))
        } else {
            "No supported L99 keyboard found".into()
        };
        self.screen_connected = panel_usb && !serial_ports.is_empty();
        self.screen_status = if self.screen_connected {
            format!("USB screen detected · /dev/{}", serial_ports.join(", /dev/"))
        } else if panel_usb {
            "Screen USB device found (EEEF:268A), but no ttyACM port is available".into()
        } else {
            "No L99 screen found (expected USB EEEF:268A)".into()
        };
        if self.notice != "Scanning for the AULA L99 keyboard" { return; }
        self.notice = if self.connected && self.screen_connected {
            "Keyboard and touchscreen detected".into()
        } else if self.connected {
            format!("{} · {}", self.device_status, self.screen_status)
        } else {
            self.device_status.clone()
        };
    }
    fn save(&mut self) {
        if let Some(p) = settings_path() {
            if let Some(parent) = p.parent() { let _ = fs::create_dir_all(parent); }
            if let Ok(data) = serde_json::to_vec_pretty(&self.settings) { let _ = fs::write(p, data); }
        }
    }
    fn colors(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        let v = &mut visuals.widgets;
        v.noninteractive.bg_fill = Color32::from_rgb(27, 31, 40);
        v.inactive.bg_fill = Color32::from_rgb(39, 45, 57);
        v.hovered.bg_fill = Color32::from_rgb(51, 60, 75);
        v.active.bg_fill = Color32::from_rgb(57, 128, 210);
        v.noninteractive.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(205, 213, 224));
        visuals.selection.bg_fill = Color32::from_rgb(43, 132, 220);
        visuals.window_fill = Color32::from_rgb(24, 27, 34);
        visuals.panel_fill = Color32::from_rgb(19, 22, 28);
        visuals.extreme_bg_color = Color32::from_rgb(15, 17, 22);
        ctx.set_visuals(visuals);
    }
    fn panel(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::group(ui.style()).fill(Color32::from_rgb(25, 29, 37)).stroke(Stroke::new(1.0_f32, Color32::from_rgb(43, 49, 61))).corner_radius(10.0).inner_margin(16.0).show(ui, |ui| {
            ui.label(RichText::new(title).size(16.0).strong().color(Color32::from_rgb(236, 240, 247)));
            ui.add_space(12.0); add(ui);
        });
    }
    fn action(&mut self, label: &str) {
        self.notice = if self.connected {
            format!("{label}: device detected, but hardware commands are unavailable in this build")
        } else {
            format!("{label}: connect the L99 keyboard and scan again")
        };
    }
    fn key_grid(&mut self, ui: &mut egui::Ui) {
        // Coordinates measured in key units from the supplied L99 product photo.
        let width = ui.available_width();
        let unit = width / 20.0;
        let (board, _) = ui.allocate_exact_size(Vec2::new(width,unit*7.1),egui::Sense::hover());
        ui.painter().rect_filled(board,8.0,Color32::from_rgb(57,63,73));
        for (index, [x,y,w,h]) in KEY_RECTS.iter().copied().enumerate() {
            let scale=width/800.0;
            let rect=egui::Rect::from_min_size(board.min+Vec2::new(x*scale,y*scale),Vec2::new(w*scale,h*scale));
            let chosen=index==self.settings.selected_key;
            let fill=if chosen {Color32::from_rgb(43,132,220)}else{Color32::from_rgb(36,43,55)};
            ui.painter().rect_filled(rect,3.0,fill);
            ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,KEYS[index],egui::FontId::proportional((unit*0.24).max(7.0)),Color32::WHITE);
            if ui.interact(rect,ui.id().with(index),egui::Sense::click()).clicked(){self.settings.selected_key=index;}
        }
        let lcd=egui::Rect::from_min_size(board.min+Vec2::new(width*655.0/800.0,width*83.0/800.0),Vec2::new(width*120.0/800.0,width*182.0/800.0));
        ui.painter().rect_filled(lcd,3.0,Color32::from_rgb(8,14,23));
        ui.painter().text(lcd.center_top()+Vec2::new(0.0,unit*0.7),egui::Align2::CENTER_CENTER,screen_clock(),egui::FontId::proportional(unit*0.5),Color32::WHITE);
        for r in 0..5 {for c in 0..4 {
            let cell=egui::Rect::from_min_size(lcd.min+Vec2::new((0.18+c as f32*0.68)*unit,(1.3+r as f32*0.66)*unit),Vec2::splat(unit*0.57));
            ui.painter().rect_filled(cell,2.0,if c==3 {Color32::from_rgb(160,118,55)}else{Color32::from_rgb(69,91,111)});
            ui.painter().text(cell.center(),egui::Align2::CENTER_CENTER,["AC","&","%","/","7","8","9","*","4","5","6","+","1","2","3","-","0",".","=","="][r*4+c],egui::FontId::proportional(unit*0.24),Color32::WHITE);
        }}
    }
    fn choose_media(&mut self, ctx: &egui::Context) {
        let output=Command::new("kdialog").args(["--getopenfilename", "", "*.png *.jpg *.jpeg *.gif|Images and animations"]).output()
            .or_else(|_|Command::new("zenity").args(["--file-selection","--title=Choose wallpaper or GIF"]).output());
        match output {
            Ok(o) if o.status.success()=> {
                self.settings.image_path=String::from_utf8_lossy(&o.stdout).trim().to_owned();
                self.load_preview(ctx);
            }, Ok(_)=>{},Err(_)=>self.notice="Paste an image or GIF path below, then press Load preview".into()
        }
    }
    fn load_preview(&mut self, ctx: &egui::Context) {
        if !cfg!(target_os = "linux") { self.notice="Media helpers are available on Linux only".into(); return; }
        match Command::new("/usr/bin/python3").args(["-I","-c",include_str!("panel.py"),"preview",&self.settings.image_path]).output() {
            Ok(o) if o.status.success()=>if let Ok(im)=image::load_from_memory(&o.stdout){let im=im.to_rgba8();self.preview=Some(ctx.load_texture("media",egui::ColorImage::from_rgba_unmultiplied([im.width() as usize,im.height() as usize],im.as_raw()),Default::default()));self.settings.screen_view=2;self.notice="Preview loaded · Send to LCD to upload".into();},
            Ok(o)=>self.notice=format!("Preview failed: {}",String::from_utf8_lossy(&o.stderr).trim()),Err(e)=>self.notice=e.to_string()
        }
    }
    fn upload_media(&mut self) {
        if !cfg!(target_os = "linux") { self.notice="Media helpers are available on Linux only".into(); return; }
        let path=self.settings.image_path.clone();
        let target=["photo-frame","background","gif"][self.settings.upload_target.min(2)];
        let (tx,rx)=std::sync::mpsc::channel();self.upload=Some(rx);
        self.notice="Encoding and uploading…".into();
        std::thread::spawn(move || {
            use std::io::{BufRead,BufReader};
            let result=(||->Result<(),String>{
                let mut child=Command::new("/usr/bin/python3").args(["-I","-u","-c",include_str!("panel.py"),target,&path]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|e|e.to_string())?;
                if let Some(out)=child.stdout.take(){for line in BufReader::new(out).lines().map_while(Result::ok){let _=tx.send(line);}}
                let out=child.wait_with_output().map_err(|e|e.to_string())?;
                if !out.status.success(){return Err(String::from_utf8_lossy(&out.stderr).trim().into());}Ok(())
            })();
            if let Err(e)=result {let _=tx.send(format!("Upload failed: {e}"));}
        });
    }
    fn content(&mut self, ui: &mut egui::Ui) {
        match self.page {
            0 => self.device(ui), 1 => self.keyboard(ui), 2 => self.lighting(ui), 3 => self.key_mapping(ui), 4 => self.touchscreen(ui), 5 => self.music(ui), 6 => self.config(ui), 7 => self.firmware(ui), 8 => self.lcd_guide(ui), _ => self.software(ui),
        }
    }
    fn device(&mut self, ui: &mut egui::Ui) {
        ui.heading("Device"); ui.label(RichText::new("Connect and manage your L99 hardware").color(Color32::GRAY)); ui.add_space(18.0);
        ui.columns(2, |c| {
            Self::panel(&mut c[0], "Keyboard", |ui| {
                ui.horizontal(|ui| { ui.label(RichText::new("●").color(if self.connected { Color32::GREEN } else { Color32::from_rgb(238,166,63) })); ui.label(if self.connected { "Connected" } else { "Not connected" }); });
                ui.add_space(8.0); ui.label("Wired USB keyboard · HID interface");
                ui.label(RichText::new(&self.device_status).size(12.0).color(if self.connected { Color32::GREEN } else { Color32::GRAY })); ui.add_space(14.0);
                if ui.button("⟳   Scan for devices").clicked() { self.scan_keyboard(); }
            });
            Self::panel(&mut c[1], "Touchscreen", |ui| {
                ui.horizontal(|ui| { ui.label(RichText::new("●").color(if self.screen_connected { Color32::GREEN } else { Color32::from_rgb(238,166,63) })); ui.label(if self.screen_connected { "Connected" } else { "Not connected" }); });
                ui.add_space(8.0); ui.label("USB serial display · photo frame and background");
                ui.label(RichText::new(&self.screen_status).size(12.0).color(if self.screen_connected { Color32::GREEN } else { Color32::GRAY }));
                ui.add_space(14.0);
                if ui.button("⟳   Scan for screens").clicked() { self.scan_keyboard(); }
            });
        });
        ui.add_space(14.0);
        Self::panel(ui,"Connection notes",|ui| {
            ui.label("Linux needs a permission rule before Live data can open the keyboard.");
            ui.add_space(6.0);
            ui.label(RichText::new("From the project folder, run these commands in a terminal (administrator password required):").size(12.0).color(Color32::GRAY));
            ui.add_space(4.0);
            ui.label(RichText::new("sh app/install-permissions.sh").monospace().size(11.0));
            ui.add_space(4.0);
            ui.label("This replaces the old 90- rule with a 70- rule and refreshes connected devices. If your terminal is inside app/, run sh install-permissions.sh.");
            ui.label("Try Live data again. If access is still denied, unplug and reconnect the keyboard. Use your local desktop session so Linux can grant you access.");
        });
    }
    fn keyboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("Keyboard"); ui.label(RichText::new("Quick controls for your L99").color(Color32::GRAY)); ui.add_space(18.0);
        self.key_grid(ui);
        ui.add_space(14.0);
        ui.columns(2, |c| {
            Self::panel(&mut c[0],"Keyboard color",|ui| { ui.label("Set every key to one color"); let mut col=Color32::from_rgba_unmultiplied(self.settings.color[0],self.settings.color[1],self.settings.color[2],self.settings.color[3]); ui.color_edit_button_srgba(&mut col); self.settings.color=[col.r(),col.g(),col.b(),col.a()]; ui.add_space(10.0); if ui.button("Apply to keyboard").clicked(){self.notice=match send_keyboard_color([self.settings.color[0],self.settings.color[1],self.settings.color[2]]){Ok(())=>"Keyboard color updated".into(),Err(e)=>format!("Color update failed: {e}")};} });
            Self::panel(&mut c[1],"On-board clock",|ui| { ui.label("The panel clock follows your computer time."); ui.add_space(10.0); if ui.button("Set clock from this computer").clicked(){self.notice=match send_live_rtc(&self.live_metrics, self.settings.cpu_thermometer){Ok(())=>"Keyboard clock synchronized".into(),Err(e)=>format!("Clock sync failed: {e}")};} ui.add_space(12.0); ui.label("System information can also be sent to the display."); });
        });
    }
    fn lighting(&mut self, ui: &mut egui::Ui) {
        ui.heading("Lighting"); ui.label(RichText::new("Built-in keyboard effects").color(Color32::GRAY)); ui.add_space(18.0);
        Self::panel(ui,"Effect",|ui| { egui::ComboBox::from_id_salt("effect").selected_text(format!("{:02}  {}", self.settings.effect+1,EFFECTS[self.settings.effect])).width(250.0).show_ui(ui,|ui| for (i,e) in EFFECTS.iter().enumerate(){ui.selectable_value(&mut self.settings.effect,i,format!("{:02}  {e}",i+1));});
            ui.checkbox(&mut self.settings.rgb_spectrum,"RGB spectrum (all colors)");
            if self.settings.rgb_spectrum { ui.label("Uses the keyboard’s built-in Spectrum effect. Turn off to use the selected effect and color."); }
            ui.add_space(15.0); ui.add(egui::Slider::new(&mut self.settings.speed,0..=100).text("Speed")); ui.add(egui::Slider::new(&mut self.settings.brightness,0..=100).text("Brightness")); let mut col=Color32::from_rgb(self.settings.color[0],self.settings.color[1],self.settings.color[2]); ui.horizontal(|ui|{ui.label("Single color"); ui.color_edit_button_srgba(&mut col);}); self.settings.color=[col.r(),col.g(),col.b(),255];
        });
        ui.add_space(14.0); ui.horizontal(|ui| { if ui.button("Apply effect").clicked(){self.notice=match send_lighting(&self.settings){Ok(())=>"Lighting updated".into(),Err(e)=>format!("Lighting update failed: {e}")};} if ui.button("Turn lighting off").clicked(){self.settings.effect=19;self.notice=match send_lighting(&self.settings){Ok(())=>"Lighting turned off".into(),Err(e)=>format!("Lighting update failed: {e}")};} });
    }
    fn key_mapping(&mut self, ui: &mut egui::Ui) {
        ui.heading("Key Mapping");
        ui.label("Give a key a new function · applies to the wired L99 on this computer");
        ui.add_space(12.0);
        self.key_grid(ui);
        let active = self.mapping_child.is_some();
        ui.add_enabled_ui(!active, |ui| {
            Self::panel(ui,"Turkish / English typing",|ui| {
                if ui.checkbox(&mut self.settings.shift2_at,"Shift + 2 → @ (Turkish Q)").changed() && self.settings.shift2_at { self.settings.ctrl_alt_altgr = false; }
                ui.label("Enable, then Apply mappings. Either Shift + 2 types @ on Turkish Q; Enter stays available.");
                ui.label("Modifiers are restored immediately after @. Fn + Q is handled inside the keyboard.");
            });
            Self::panel(ui,"Selected key",|ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("Selected: {}",KEYS[self.settings.selected_key]));
                    if ui.add_enabled(self.settings.selected_key != 79,egui::Button::new("Add mapping")).clicked() {
                        if !self.settings.mappings.iter().any(|m|m.key==self.settings.selected_key) {self.settings.mappings.push(KeyMapping {key:self.settings.selected_key,target:0});}
                    }
                    if ui.button("Right Ctrl → AltGr").clicked() {
                        self.settings.mappings.retain(|m|m.key!=80);
                        self.settings.mappings.push(KeyMapping {key:80,target:0});
                    }
                });
                ui.label("Fn is usually handled inside the keyboard and cannot be detected by Linux. Use Right Ctrl as AltGr, then press it with Q for @ on compatible layouts.");
                ui.horizontal(|ui| {
                    ui.label("Your layout types @ with:");
                    ui.radio_value(&mut self.settings.at_shift2,false,"AltGr + Q");
                    ui.radio_value(&mut self.settings.at_shift2,true,"Shift + 2 (US)");
                });
                let mut remove=None;
                for (i,m) in self.settings.mappings.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} →",KEYS.get(m.key).unwrap_or(&"Unknown")));
                        let label=match m.target {0=>"AltGr (Right Alt)",1=>"@",2=>"Disabled",n=>KEYS.get(n-3).copied().unwrap_or("Unknown")};
                        egui::ComboBox::from_id_salt(("map",i)).selected_text(label).show_ui(ui,|ui| {
                            ui.selectable_value(&mut m.target,0,"AltGr (Right Alt)");
                            ui.selectable_value(&mut m.target,1,"@ (layout shortcut)");
                            ui.selectable_value(&mut m.target,2,"Disabled");
                            for (k,label) in KEYS.iter().enumerate() {if k!=79 {ui.selectable_value(&mut m.target,k+3,*label);}}
                        });
                        if ui.button("Remove").clicked(){remove=Some(i);}
                    });
                }
                if let Some(i)=remove {self.settings.mappings.remove(i);}
            });
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.add_enabled(cfg!(target_os = "linux")&&!active&&(self.settings.shift2_at||self.settings.ctrl_alt_altgr||!self.settings.mappings.is_empty()),egui::Button::new("Apply mappings…")).clicked() {
                self.save();
                let result=Command::new("/usr/bin/pkexec").arg("/usr/bin/python3").arg("-I").arg("-u").arg("-c").arg(include_str!("remap.py")).arg(mapping_payload(&self.settings))
                    .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn();
                match result {
                    Ok(mut child)=>{
                        use std::io::BufRead;
                        let (tx,rx)=std::sync::mpsc::channel();
                        for stream in [child.stdout.take().map(|s|Box::new(s) as Box<dyn std::io::Read+Send>),child.stderr.take().map(|s|Box::new(s) as Box<dyn std::io::Read+Send>)].into_iter().flatten() {
                            let tx=tx.clone();std::thread::spawn(move ||{for line in std::io::BufReader::new(stream).lines().map_while(Result::ok){let _=tx.send(line);}});
                        }
                        self.mapping_child=Some(child);self.mapping_messages=Some(rx);self.notice="Waiting for Linux authentication to enable key mapping…".into();
                    },
                    Err(e)=>self.notice=format!("Cannot start mapping: {e}. Python 3 and PolicyKit are required."),
                }
            }
            if ui.add_enabled(active,egui::Button::new("Stop / restore normal keys")).clicked() {if let Some(child)=&mut self.mapping_child {child.stdin.take();}self.notice="Stopping mappings…".into();}
            if ui.button("Save mappings").clicked(){self.save();self.notice="Mappings saved; use Apply mappings to activate".into();}
        });
        ui.label("Linux will ask for administrator authentication to access keyboard input. Mappings stop when you close the app and are never written to keyboard firmware.");
    }
    fn touchscreen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Touchscreen");
        ui.label("Choose the matching page on the keyboard LCD to view uploaded content or live data.");
        if ui.button("LCD features & what is missing").clicked() { self.page=8; }
        self.key_grid(ui);
        ui.horizontal(|ui| {
            if ui.button("Choose image or GIF…").clicked(){self.choose_media(ui.ctx());}
            ui.text_edit_singleline(&mut self.settings.image_path);
            if ui.button("Load preview").clicked(){self.load_preview(ui.ctx());}
        });
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("destination").selected_text(["Photo frame","Wallpaper","Customized animation"][self.settings.upload_target.min(2)]).show_ui(ui,|ui|{for (i,t) in ["Photo frame","Wallpaper","Customized animation"].iter().enumerate(){ui.selectable_value(&mut self.settings.upload_target,i,*t);}});
            if ui.add_enabled(self.upload.is_none()&&!self.settings.image_path.is_empty(),egui::Button::new("Send to LCD")).clicked(){self.upload_media();}
            if ui.button(if self.realtime_enabled {"Stop live data"}else{"Digital clock + live data"}).clicked(){self.realtime_enabled=!self.realtime_enabled;self.last_metrics_update=Instant::now()-Duration::from_secs(1);}
        });
        Self::panel(ui, "Digital clock · live preview", |ui| {
            let time = Command::new("date").args(["+%H:%M:%S"]).output().ok()
                .and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_else(|| "--:--:--".into());
            ui.label(RichText::new(time.trim()).monospace().size(32.0).color(Color32::from_rgb(75,210,245)));
            for (label, load, temperature) in [("CPU",Some(self.live_metrics.cpu_load),self.live_metrics.cpu_temp),("GPU",self.live_metrics.gpu_load,self.live_metrics.gpu_temp)] {
                let text=format!("{label}  {}  ·  {}",load.map(|v|format!("{v}%")).unwrap_or_else(||"unavailable".into()),temperature.map(|v|format!("{v}°C")).unwrap_or_else(||"temperature unavailable".into()));
                ui.add(egui::ProgressBar::new(load.unwrap_or(0) as f32/100.0).text(text));
            }
        });
        if ui.checkbox(&mut self.settings.cpu_thermometer,"Show CPU temperature in the LCD thermometer (°C)").changed() {
            self.last_metrics_update=Instant::now()-Duration::from_secs(1);
            self.save();
        }
        ui.label("The small thermometer now represents CPU temperature when enabled. Humidity is not collected; its 0% is not CPU load.");
        ui.label(format!("CPU {}% · {}    GPU {} · {}",self.live_metrics.cpu_load,self.live_metrics.cpu_temp.map(|v|format!("{v}°C")).unwrap_or_else(||"temperature unavailable".into()),self.live_metrics.gpu_load.map(|v|format!("{v}%")).unwrap_or_else(||"unavailable".into()),self.live_metrics.gpu_temp.map(|v|format!("{v}°C")).unwrap_or_else(||"unavailable".into())));
        let sensors=temperature_sensors(std::path::Path::new("/sys/class/hwmon"));
        ui.horizontal(|ui| {
            ui.label("CPU temperature sensor");
            let selected=if self.settings.cpu_sensor.is_empty(){"Automatic (CPU package)".to_owned()}else{sensors.iter().find(|s|s.id==self.settings.cpu_sensor).map(|s|s.label.clone()).unwrap_or_else(||"Saved sensor unavailable".into())};
            egui::ComboBox::from_id_salt("cpu-sensor").selected_text(selected).show_ui(ui,|ui| {
                if ui.selectable_value(&mut self.settings.cpu_sensor,String::new(),"Automatic (CPU package)").changed(){self.last_metrics_update=Instant::now()-Duration::from_secs(1);}
                for sensor in &sensors {if ui.selectable_value(&mut self.settings.cpu_sensor,sensor.id.clone(),&sensor.label).changed(){self.last_metrics_update=Instant::now()-Duration::from_secs(1);}}
            });
            if ui.button("Save sensor").clicked(){self.save();self.notice="Temperature sensor saved".into();}
        });
        ui.label("Choose your CPU package/Tdie sensor if Automatic is unavailable or incorrect.");
        ui.label(&self.realtime_status);
        ui.label("On the LCD: Home → Real-time Data for the built-in digital clock and live readings. Keep this app running and live data enabled.");
        ui.label("Customized animation plays a stored GIF; its clock design cannot be edited into a real-time screen by USB. Use Real-time Data for an accurate clock and current readings.");
        ui.horizontal(|ui|{
            if let Some(texture)=&self.preview {ui.add(egui::Image::new(texture).fit_to_exact_size(Vec2::new(80.0,120.0)));}
            ui.vertical(|ui|{
                ui.add_enabled(false, egui::Slider::new(&mut self.settings.screen_brightness,0..=100).text("LCD brightness"));
                ui.label("Adjust brightness on the LCD itself; USB control is unavailable.");
                ui.label("USB brightness and screensaver selection commands are not documented; the app cannot change them yet.");
            });
        });
    }
    fn firmware(&mut self, ui: &mut egui::Ui) {
        ui.heading("Firmware Update");
        ui.label("Keyboard and LCD firmware are separate updates in the original Windows driver.");
        ui.add_space(12.0);
        Self::panel(ui, "Linux update status", |ui| {
            ui.label(RichText::new("Flashing is not available in this Linux app yet").strong());
            ui.label("The firmware transfer and recovery protocols have not been verified. Image uploads and live data do not update firmware.");
            ui.label("Installed device firmware versions: unavailable. Bundled updater filenames are not a reading of your keyboard's version.");
        });
        ui.add_space(12.0);
        Self::panel(ui, "Windows firmware instructions", |ui| {
            ui.label("AULA's guide requires a Windows PC and a wired USB connection. Obtain the firmware package for your exact L99 hardware from AULA before starting.");
            ui.label("Update the keyboard first with the keyboard reset tool. For the screen, the guide describes holding Fn and pressing X, 0, 6 before running the separate screen updater.");
            ui.label("Wait for each tool to confirm completion. Do not disconnect USB, close the updater or power off during flashing; reconnect only after completion.");
            ui.hyperlink_to("Open AULA firmware update guide", "https://aulastar.com/announcement/788.html");
            ui.label("This app does not flash firmware on Linux or macOS. Installed firmware version detection is unavailable.");
        });
        ui.add_space(12.0);
        if ui.button("See LCD capabilities and missing features").clicked() { self.page=8; }
    }
    fn software(&mut self, ui: &mut egui::Ui) {
        ui.heading("Software & manuals");
        ui.label("Choose software for the computer you use with your L99.");
        ui.add_space(12.0);
        Self::panel(ui, "Windows", |ui| {
            ui.label("This project includes a Windows desktop build script. USB controls still require Linux in this Rust app. For the vendor Windows controls, use the separate official driver below.");
            ui.hyperlink_to("Open AULA L99 Windows driver download page", "https://aulagear.com/blogs/software/aula-l99-driver");
        });
        ui.add_space(12.0);
        Self::panel(ui, "Mac / macOS", |ui| {
            ui.label("This project includes a macOS desktop build script. The desktop preview and saved configurations are portable; USB lighting, remapping, LCD uploads and live monitoring still require Linux. Native releases have not been verified here.");
            ui.label("Keyboard compatibility with Mac does not establish support for the configuration software. See the manual for the keyboard's own controls and connection modes.");
        });
        ui.add_space(12.0);
        Self::panel(ui, "Manual & firmware", |ui| {
            ui.hyperlink_to("Open AULA L99 manual (PDF)", "https://aulagear.com/cdn/shop/files/AULA_L99_Manual.pdf?v=18380816096559756908");
            ui.hyperlink_to("Open AULA firmware update guide (Windows)", "https://aulastar.com/announcement/788.html");
        });
    }
    fn lcd_guide(&mut self, ui: &mut egui::Ui) {
        ui.heading("LCD guide · capabilities & gaps");
        ui.label("Available means implemented in this app; physical LCD behavior still needs confirmation on your keyboard.");
        ui.add_space(10.0);
        egui::Grid::new("lcd-capabilities").num_columns(3).striped(true).spacing([18.0,12.0]).show(ui, |ui| {
            ui.strong("Feature"); ui.strong("Status"); ui.strong("How to use / what is missing"); ui.end_row();
            for (feature,status,how) in [
                ("Digital clock + date", "Available", "Touchscreen → Digital clock + live data; LCD Home → Real-time Data. Keep the app open."),
                ("CPU load + temperature", "Available", "Enable live data; select a CPU package/Tdie sensor on Touchscreen if needed."),
                ("GPU load + temperature", "Driver dependent", "Uses NVIDIA tools or Linux DRM/hwmon. Missing sensors cannot supply readings."),
                ("Small thermometer", "CPU option", "Enable Show CPU temperature to mirror Celsius here; this replaces the weather temperature."),
                ("Photo frame / wallpaper", "Available", "Choose image → select Photo frame or Wallpaper → Send to LCD; select that page on the LCD."),
                ("Customized animation", "Available", "Choose GIF → Customized animation → Send to LCD. Up to 200 frames with uniform timing."),
                ("Calculator", "On the LCD", "Use the keyboard screen's calculator page, as shown in the reference photo; desktop control is unavailable."),
                ("Brightness / screensaver", "LCD only", "Adjust on the physical screen. USB control and automatic page selection are missing."),
                ("Weather + humidity", "Missing", "No weather service is connected. Humidity is unset; 0% is not CPU load."),
                ("Custom live clock design", "Missing", "Stored GIFs cannot show live time or metrics. The built-in Real-time Data layout is fixed."),
                ("Firmware updates", "Windows only", "Separate keyboard/screen updaters exist; Linux flashing and version detection are missing."),
                ("Music / audio rhythm", "Missing", "Audio capture and live rhythm streaming are not implemented; the Music page is a preview."),
            ] {
                ui.label(feature); ui.label(status);
                ui.add_sized([460.0, 0.0], egui::Label::new(how).wrap()); ui.end_row();
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Open Touchscreen controls").clicked() { self.page=4; }
            if ui.button("Open Firmware Update").clicked() { self.page=7; }
        });
    }
    fn music(&mut self, ui: &mut egui::Ui) {
        ui.heading("Music"); ui.label(RichText::new("Audio visualizer and panel rhythm options").color(Color32::GRAY)); ui.add_space(18.0);
        Self::panel(ui,"Spectrum",|ui| {ui.label("Audio capture is ready to configure when an input device is available.");ui.add_space(16.0);ui.horizontal(|ui| for i in 0..17 { let h=12.0+((i*37+19)%55) as f32; let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 70.0), egui::Sense::hover()); let bar = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.bottom()-h), Vec2::new(20.0,h)); ui.painter().rect_filled(bar, 3.0, Color32::from_rgb(50,165,230)); });ui.add_space(16.0);if ui.button("Start audio visualizer").clicked(){self.action("Audio visualizer");}});
        ui.add_space(14.0); Self::panel(ui,"Music Rhythm",|ui| {ui.horizontal(|ui| {ui.label("Rhythm");egui::ComboBox::from_id_salt("rhythm").selected_text("Rhythm 1").show_ui(ui,|ui|for i in 1..=15{let _=ui.selectable_label(i==1,format!("Rhythm {i}"));});});ui.horizontal(|ui|{ui.label("Background mode");egui::ComboBox::from_id_salt("bgmode").selected_text("Mode 1").show_ui(ui,|ui|for i in 1..=15{let _=ui.selectable_label(i==1,format!("Mode {i}"));});});let mut a=50;ui.add(egui::Slider::new(&mut a,0..=100).text("Amplitude"));let mut b=45;ui.add(egui::Slider::new(&mut b,0..=100).text("Background brightness"));});
    }
    fn config(&mut self, ui: &mut egui::Ui) {
        ui.heading("Config"); ui.label(RichText::new("Keyboard response and application preferences").color(Color32::GRAY)); ui.add_space(18.0);
        Self::panel(ui,"Keyboard settings",|ui| {ui.horizontal(|ui|{ui.label("Response time");egui::ComboBox::from_id_salt("response").selected_text(["Level 1","Level 2","Level 3","Level 4","Level 5"][self.settings.response]).show_ui(ui,|ui|for (i,s) in ["Level 1","Level 2","Level 3","Level 4","Level 5"].iter().enumerate(){ui.selectable_value(&mut self.settings.response,i,*s);});});ui.horizontal(|ui|{ui.label("Sleep timer");egui::ComboBox::from_id_salt("sleep").selected_text(["Never","1 minute","5 minutes","30 minutes"][self.settings.sleep]).show_ui(ui,|ui|for (i,s) in ["Never","1 minute","5 minutes","30 minutes"].iter().enumerate(){ui.selectable_value(&mut self.settings.sleep,i,*s);});});if ui.button("Apply keyboard settings").clicked(){self.notice=match send_keyboard_settings(&self.settings){Ok(())=>"Keyboard settings updated".into(),Err(e)=>format!("Settings update failed: {e}")};}});
        ui.add_space(14.0); Self::panel(ui,"Application",|ui| {ui.label("Your lighting and device preferences are saved on this computer.");if ui.button("Save preferences").clicked(){self.save();self.notice="Preferences saved".into();}});
    }
    fn save_capture(&mut self, ctx: &egui::Context) {
        if let Some(path)=self.screenshot.clone() {
            self.frame_count+=1;
            if self.frame_count>=120 && !self.capture_requested {ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::UserData::default()));self.capture_requested=true;}
            let images=ctx.input(|i| i.raw.events.iter().filter_map(|e| if let egui::Event::Screenshot{image,..}=e {Some(image.clone())}else{None}).collect::<Vec<_>>());
            if let Some(image)=images.first(){
                let mut rgba=Vec::with_capacity(image.pixels.len()*4);
                for p in &image.pixels {rgba.extend_from_slice(&[p.r(),p.g(),p.b(),p.a()]);}
                if let Some(parent)=path.parent(){let _=fs::create_dir_all(parent);}
                let _=image::save_buffer(path,&rgba,image.width() as u32,image.height() as u32,image::ColorType::Rgba8);
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}
impl eframe::App for DriverApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        Self::colors(ctx);
        if let Some(rx)=&self.mapping_messages {while let Ok(message)=rx.try_recv(){self.notice=message;}}
        if let Some(child)=&mut self.mapping_child {if let Ok(Some(status))=child.try_wait(){self.mapping_child=None;self.mapping_messages=None;if status.success(){self.notice="Mappings stopped; normal keyboard restored".into();}else{self.notice=format!("Mapping stopped ({status}). {}",self.notice);}}}

        if let Some(rx)=&self.upload {
            let mut finished=false;
            loop {match rx.try_recv(){Ok(message)=>self.notice=message,Err(std::sync::mpsc::TryRecvError::Disconnected)=>{finished=true;break;},Err(_)=>break,}}
            if finished {self.upload=None;}
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.last_device_scan.elapsed() >= Duration::from_secs(2) { self.scan_keyboard(); }
        if self.last_metrics_update.elapsed() >= Duration::from_secs(1) {
            if let Some((total, idle)) = cpu_counters() {
                if let Some((old_total, old_idle)) = self.previous_cpu {
                    let dt = total.saturating_sub(old_total); let di = idle.saturating_sub(old_idle);
                    if dt > 0 { self.live_metrics.cpu_load = (((dt.saturating_sub(di)) as f64 / dt as f64) * 100.0).round().clamp(0.0,100.0) as u8; }
                }
                self.previous_cpu = Some((total, idle));
            }
            self.live_metrics.cpu_temp = selected_cpu_temperature(&self.settings.cpu_sensor);
            (self.live_metrics.gpu_load,self.live_metrics.gpu_temp)=gpu_metrics();
            if self.realtime_enabled {
                self.realtime_status = match send_live_rtc(&self.live_metrics, self.settings.cpu_thermometer) { Ok(()) => "Live data sent to the keyboard · refreshing every second".into(), Err(e) => format!("Live data not sent: {e}") };
            }
            self.last_metrics_update = Instant::now();
        }
        ctx.request_repaint_after(Duration::from_secs(1));
        egui::TopBottomPanel::top("top").exact_height(64.0).show(ctx,|ui|{
            ui.horizontal(|ui| {ui.add_space(12.0);ui.label(RichText::new("AULA L99").size(20.0).strong().color(Color32::WHITE));ui.label(RichText::new(format!("{} CONTROL", platform_name().to_uppercase())).size(10.0).color(Color32::from_rgb(75,177,245)));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.label(RichText::new("●  ").color(if self.connected{Color32::GREEN}else{Color32::from_rgb(238,166,63)}));ui.label(if self.connected{"Device connected"}else{"No device connected"});});});
        });
        egui::SidePanel::left("navigation").exact_width(188.0).resizable(false).show(ctx,|ui|{
            ui.add_space(20.0);ui.label(RichText::new("WORKSPACE").size(10.0).strong().color(Color32::from_rgb(117,128,147)));ui.add_space(8.0);
            for (i,name) in NAV.iter().enumerate(){let selected=self.page==i;let text=RichText::new(format!("{}     {}",["▣","⌨","✦","▦","▤","♫","⚙","↑","?","↗"][i],name)).size(14.0).color(if selected{Color32::WHITE}else{Color32::from_rgb(170,180,195)});if ui.add(egui::Button::new(text).fill(if selected{Color32::from_rgb(41,74,111)}else{Color32::TRANSPARENT}).corner_radius(7.0).min_size(Vec2::new(166.0,38.0))).clicked(){self.page=i;}}
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min),|ui|{ui.add_space(16.0);ui.label(RichText::new(format!("AULA L99 · {}", platform_name())).size(11.0).color(Color32::GRAY));ui.label(RichText::new("Desktop controls").size(10.0).color(Color32::DARK_GRAY));});
        });
        egui::CentralPanel::default().show(ctx,|ui|{
            egui::ScrollArea::vertical().auto_shrink([false,false]).show(ui,|ui|{ui.add_space(20.0);ui.add_space(10.0);if !cfg!(target_os = "linux") { ui.colored_label(Color32::YELLOW, "Desktop preview: USB controls, live readings and remapping are not yet available on this platform."); ui.add_space(10.0); } self.content(ui);ui.add_space(30.0);});
        });
        egui::TopBottomPanel::bottom("status").exact_height(30.0).show(ctx,|ui|{ui.horizontal(|ui|{ui.add_space(10.0);ui.label(RichText::new("●").color(Color32::from_rgb(238,166,63)));ui.label(RichText::new(&self.notice).size(11.0).color(Color32::from_rgb(160,170,184)));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.label(RichText::new("L99  ·  USB HID").size(10.0).color(Color32::DARK_GRAY));});});});
        self.save_capture(ctx);
        let _elapsed = self.started.elapsed();
    }
}
fn main() -> eframe::Result<()> {
    let mut args=std::env::args().skip(1);let mut screenshot=None;
    while let Some(arg)=args.next(){if arg=="--screenshot" {screenshot=args.next().map(PathBuf::from);}}
    let capture=screenshot.is_some();
    let mut viewport=egui::ViewportBuilder::default().with_inner_size([1120.0,760.0]).with_min_inner_size([900.0,650.0]).with_title(format!("AULA L99 · {}", platform_name()));
    if capture {viewport=viewport.with_active(false);}
    let options=eframe::NativeOptions{viewport,..Default::default()};
    eframe::run_native("AULA L99",options,Box::new(move |_cc|Ok(Box::new(DriverApp::new(screenshot)))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_settings_are_bounded_before_use() {
        let mut settings = Settings::default();
        settings.effect = usize::MAX;
        settings.response = usize::MAX;
        settings.selected_key = usize::MAX;
        settings.sleep = usize::MAX;
        settings.mappings = vec![KeyMapping {key: usize::MAX, target: 0}, KeyMapping {key: 16, target: usize::MAX}];
        settings.validate();
        assert_eq!(settings.effect, 19);
        assert_eq!(settings.response, 4);
        assert_eq!(settings.selected_key, 83);
        assert_eq!(settings.sleep, 3);
        assert!(settings.mappings.is_empty());
    }
    #[test]
    fn mapping_payload_uses_evdev_codes_and_layout() {
        let mut settings=Settings::default();
        settings.mappings=vec![KeyMapping{key:80,target:0},KeyMapping{key:33,target:1},KeyMapping{key:79,target:0}];
        assert_eq!(mapping_payload(&settings),"[[97,[100]],[16,[100,16]]]");
        settings.at_shift2=true;
        assert_eq!(mapping_payload(&settings),"[[97,[100]],[16,[42,3]]]");
    }
    #[test]
    fn temperature_prefers_die_and_rejects_invalid_readings() {
        let root=std::env::temp_dir().join(format!("l99-sensor-test-{}",std::process::id()));
        let dir=root.join("hwmon0");fs::create_dir_all(&dir).unwrap();
        for (name,value) in [("name","k10temp"),("temp1_input","75000"),("temp1_label","Tctl"),("temp2_input","65000"),("temp2_label","Tdie"),("temp3_input","-1000"),("temp4_input","garbage")] {fs::write(dir.join(name),value).unwrap();}
        let sensors=temperature_sensors(&root);
        assert_eq!(sensors.len(),2);assert_eq!(sensors[0].value,65);assert!(sensors[0].cpu);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn acknowledgements_use_flag_and_match_command() {
        let mut reply = [0; 65];
        for opcode in [0x18, 0x13, 0x23, 0x28, 0x17, 0x02, 0xf0, 0xf5] {
            reply[1..5].copy_from_slice(&[4, opcode, 0, 1]);
            for status in [1, 3, 0xff] {
                reply[4] = status;
                assert!(command_acknowledged(&reply, opcode));
            }
            for status in [0, 2, 0xfe] {
                reply[4] = status;
                assert!(!command_acknowledged(&reply, opcode));
            }
            reply[4] = 0xff;
            assert!(!command_acknowledged(&reply, opcode ^ 1));
            reply[1] = 0;
            assert!(!command_acknowledged(&reply, opcode));
        }
    }
    #[test]
    fn monitor_transmits_temperatures_and_load_separately() {
        let metrics = LiveMetrics { cpu_load: 23, cpu_temp: Some(61), gpu_load: Some(45), gpu_temp: Some(72) };
        let packet = rtc_block(&metrics, true).unwrap();
        assert_eq!(&packet[13..17], &[23, 61, 45, 72]);
        assert_eq!(packet[17],61);
        assert_eq!(&packet[18..22], &[0,0,0,0]);
        assert_eq!(rtc_block(&metrics,false).unwrap()[17],0);
        assert_eq!(rtc_block(&LiveMetrics::default(),true).unwrap()[17],0);
    }
}
