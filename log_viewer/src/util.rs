// Convert a number (usually a hash) to as succinct a set of characters
// as possible while still being unique. Do this by expanding the alphabet
// to include all alphanumeric characters and cases.
pub fn small_text(n: u64) -> String {
    let mut n = n;
    let mut s = String::new();
    let alphabet = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let base = alphabet.len() as u64;
    while n > 0 {
        let i = (n % base) as usize;
        s.push(alphabet.chars().nth(i).unwrap());
        n /= base;
    }
    s
}

pub fn trim_path(path: &str) -> &str {
    path.trim_start_matches("/root/World/")
}
