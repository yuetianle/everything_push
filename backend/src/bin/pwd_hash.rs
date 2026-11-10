use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::Argon2;

fn main() {
    let pw = std::env::args().nth(1).expect("用法: cargo run --bin pwd_hash -- <密码>");
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .unwrap()
        .to_string();
    println!("{hash}");
}
