#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let re = regex::Regex::new(r"^[a-z][a-z0-9-]{0,78}[a-z0-9]$|^[a-z0-9]$").unwrap();
        let _ = re.is_match(s);
    }
});
