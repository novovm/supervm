fn main() {
    println!("cargo:rerun-if-changed=vendor/test_quota.c");
    println!("cargo:rerun-if-changed=vendor/test_quota.h");
    cc::Build::new()
        .include(std::env::var("DEP_SQLITE3_INCLUDE").expect("bundled SQLite headers"))
        .file("vendor/test_quota.c")
        .define("SQLITE_THREADSAFE", "1")
        .warnings(false)
        .compile("novovm_sqlite_quota");
}
