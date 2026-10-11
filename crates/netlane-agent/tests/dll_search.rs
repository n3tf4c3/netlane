//! The elevated agent may sit in a user-writable folder; its static imports must never
//! be resolved beside the executable. Reads the built PE only; launches nothing.
#![cfg(all(windows, target_env = "msvc"))]

fn u16_at(image: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(image[offset..offset + 2].try_into().unwrap())
}
fn u32_at(image: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(image[offset..offset + 4].try_into().unwrap())
}

#[test]
fn agent_static_imports_are_resolved_from_system32_only() {
    let image = std::fs::read(env!("CARGO_BIN_EXE_netlane-agent")).unwrap();
    let pe = u32_at(&image, 0x3c) as usize;
    assert_eq!(&image[pe..pe + 4], b"PE\0\0");
    let coff = pe + 4;
    let sections = usize::from(u16_at(&image, coff + 2));
    let optional = coff + 20;
    assert_eq!(u16_at(&image, optional), 0x20b, "PE32+ esperado");
    // Data directory 10 (load configuration) of the PE32+ optional header.
    let rva = u32_at(&image, optional + 112 + 10 * 8);
    assert_ne!(rva, 0, "Executável sem diretório de configuração de carga.");
    let table = optional + usize::from(u16_at(&image, coff + 16));
    let offset = (0..sections)
        .map(|i| table + i * 40)
        .find_map(|header| {
            let address = u32_at(&image, header + 12);
            let size = u32_at(&image, header + 8).max(u32_at(&image, header + 16));
            (address..address + size)
                .contains(&rva)
                .then(|| (rva - address + u32_at(&image, header + 20)) as usize)
        })
        .expect("Seção da configuração de carga");
    assert!(u32_at(&image, offset) >= 80);
    // IMAGE_LOAD_CONFIG_DIRECTORY64.DependentLoadFlags (winnt.h, offset 78).
    assert_eq!(
        u16_at(&image, offset + 78),
        0x800,
        "LOAD_LIBRARY_SEARCH_SYSTEM32 ausente"
    );
}
