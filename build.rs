use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["src/data/backup.proto"], &["src/"])?;
    Ok(())
}