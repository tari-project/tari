use std::env;

fn main() {
    println!("cargo:rerun-if-changed=icon.res");
    let mut path = env::current_dir().unwrap();
    path.push("icon.res");
    println!("cargo:rustc-link-arg={}", path.into_os_string().into_string().unwrap());
}
