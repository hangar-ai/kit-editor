//! The manifest names the module, and the module is one Hangar loads: the
//! native cases of test/manifest.test.js, and what docs/native/kits-native.md
//! asks of a module (its imports and exports).

mod common;

use std::path::{Path, PathBuf};

use serde_json::Value;

fn kit() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn manifest() -> Value {
    serde_json::from_str(&std::fs::read_to_string(kit().join("hangar-kit.json")).unwrap()).unwrap()
}

#[test]
fn the_tab_keeps_its_page_and_names_its_module() {
    let manifest = manifest();
    assert_eq!(
        manifest["schema"], 1,
        "so that the kit still opens on Hangar 1.x"
    );
    assert_eq!(manifest["hangar"], ">=1.9.0");
    let tab = &manifest["tabs"][0];
    assert_eq!(tab["entry"], "tabs/editor/index.html");
    assert!(
        kit().join("tabs/editor/index.html").is_file(),
        "the page stays"
    );
    assert_eq!(tab["native"], "tabs/editor.wasm");
    let mut permissions: Vec<&str> = tab["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    permissions.sort();
    assert_eq!(
        permissions,
        common::GRANTED,
        "the page's three, which the tests' Hangar holds the tab to"
    );
}

fn leb(bytes: &[u8], at: &mut usize) -> u64 {
    let (mut value, mut shift) = (0u64, 0);
    loop {
        let byte = bytes[*at];
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}

fn name(bytes: &[u8], at: &mut usize) -> String {
    let len = leb(bytes, at) as usize;
    let text = String::from_utf8(bytes[*at..*at + len].to_vec()).unwrap();
    *at += len;
    text
}

/// The module's imports (`module.field`) and exports.
fn imports_and_exports(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    assert_eq!(
        &bytes[..8],
        b"\0asm\x01\0\0\0",
        "a WebAssembly module, version 1"
    );
    let (mut imports, mut exports) = (Vec::new(), Vec::new());
    let mut at = 8;
    while at < bytes.len() {
        let id = bytes[at];
        at += 1;
        let size = leb(bytes, &mut at) as usize;
        let end = at + size;
        let mut i = at;
        match id {
            2 => {
                for _ in 0..leb(bytes, &mut i) {
                    let module = name(bytes, &mut i);
                    let field = name(bytes, &mut i);
                    imports.push(format!("{module}.{field}"));
                    match bytes[i] {
                        0 => {
                            i += 1;
                            leb(bytes, &mut i);
                        }
                        kind => panic!("an import of kind {kind}: {module}.{field}"),
                    }
                }
            }
            7 => {
                for _ in 0..leb(bytes, &mut i) {
                    exports.push(name(bytes, &mut i));
                    i += 1;
                    leb(bytes, &mut i);
                }
            }
            _ => {}
        }
        at = end;
    }
    (imports, exports)
}

#[test]
fn the_module_imports_only_what_hangar_gives_and_exports_the_abi() {
    let path = kit().join("tabs/editor.wasm");
    let bytes = std::fs::read(&path).expect("tabs/editor.wasm: run scripts/build-native.sh");
    assert!(
        bytes.len() < 4 * 1024 * 1024,
        "a small module: {} bytes",
        bytes.len()
    );
    let (mut imports, exports) = imports_and_exports(&bytes);
    imports.sort();
    for import in &imports {
        assert!(
            ["hangar.log", "hangar.now"].contains(&import.as_str()),
            "no WASI, no glue: {import}"
        );
    }
    for export in [
        "memory",
        "hangar_abi_version",
        "hangar_alloc",
        "hangar_init",
        "hangar_update",
        "hangar_view",
    ] {
        assert!(
            exports.iter().any(|other| other == export),
            "{export} in {exports:?}"
        );
    }
}

#[test]
fn the_module_names_no_path_of_the_machine_that_built_it() {
    let bytes = std::fs::read(kit().join("tabs/editor.wasm")).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for personal in ["/home/", "/Users/", "C:\\Users"] {
        assert!(
            !text.contains(personal),
            "{personal} is in the module: build it with scripts/build-native.sh"
        );
    }
}
