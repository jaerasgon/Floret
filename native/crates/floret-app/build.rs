//! Embed the built-in samples (`native/samples/*.flora`) into the binary.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    let cat_path = dir.join("catalogue.json");
    println!("cargo:rerun-if-changed={}", cat_path.display());
    let text = std::fs::read_to_string(&cat_path).expect("samples/catalogue.json");
    let cat: serde_json::Value = serde_json::from_str(&text).expect("catalogue json");
    let mut out = String::from("pub static SAMPLES: &[Sample] = &[\n");
    for s in cat["samples"].as_array().expect("samples") {
        let file = dir.join(s["file"].as_str().expect("file"));
        println!("cargo:rerun-if-changed={}", file.display());
        writeln!(
            out,
            "    Sample {{ id: {:?}, name: {:?}, category: {:?}, description: {:?}, flora: include_str!({:?}) }},",
            s["id"].as_str().unwrap_or_default(),
            s["name"].as_str().unwrap_or_default(),
            s["category"].as_str().unwrap_or_default(),
            s["description"].as_str().unwrap_or_default(),
            file.canonicalize().expect("sample path").display().to_string(),
        )
        .unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("samples.rs");
    std::fs::write(dest, out).unwrap();

    // macOS reads an unbundled binary's Info.plist from this section; the
    // microphone and camera prompts need its usage strings (an .app bundle
    // would carry the same file as Contents/Info.plist).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = Path::new(env!("CARGO_MANIFEST_DIR")).join("macos/Info.plist");
        println!("cargo:rerun-if-changed={}", plist.display());
        println!(
            "cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            plist.display()
        );
    }
}
