// Executable identity; GPUI supplies its own application manifest.
fn main() {
    println!("cargo:rerun-if-changed=../windows-resources.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brand/netlane-diagnostics.ico");
    println!("cargo:rerun-if-changed={}",icon.display());
    let icon=icon.canonicalize().expect("NetLane icon").display().to_string().trim_start_matches(r"\\?\").replace('\\',"/");
    let version=std::env::var("CARGO_PKG_VERSION").unwrap();
    let numbers:Vec<u16>=version.split('.').map(|n|n.parse().expect("numeric version")).collect();
    let [major,minor,patch]=numbers[..] else { panic!("Three-part version required"); };
    let agent=std::env::var("CARGO_PKG_NAME").as_deref()==Ok("netlane-agent");
    let (description,filename)=if agent {("NetLane Routing Agent","netlane-agent.exe")}else{("NetLane","netlane.exe")};
    let resource=format!(r#"#include <winver.h>
1 ICON "{icon}"
VS_VERSION_INFO VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK VS_FFI_FILEFLAGSMASK
FILEFLAGS 0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
FILESUBTYPE VFT2_UNKNOWN
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "041604B0"
  BEGIN
   VALUE "FileDescription", "{description}"
   VALUE "FileVersion", "{version}"
   VALUE "OriginalFilename", "{filename}"
   VALUE "ProductName", "NetLane"
   VALUE "ProductVersion", "{version}"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x0416, 0x04B0
 END
END
"#);
    let path=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("netlane-version.rc");
    std::fs::write(&path,resource).expect("Write Win32 resources");
    embed_resource::compile(&path,embed_resource::NONE).manifest_optional().expect("Compile Win32 resources");
}
