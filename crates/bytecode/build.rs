use serde_derive::Deserialize;
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io,
    path::Path,
};

#[derive(Deserialize)]
struct BytecodeConfig {
    opcode: u8,
    format: Option<BytecodeFormat>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum BytecodeFormat {
    Wide,
    Byte,
}

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("bytecodes.rs");
    let bytecodes_toml = fs::read_to_string("bytecodes.toml").unwrap();
    let toml: HashMap<String, BytecodeConfig> = toml::from_str(&bytecodes_toml).unwrap();

    BytecodeWriter {
        w: File::create(dest_path).unwrap(),
        toml,
    }
    .write()
    .unwrap();

    println!("cargo:rerun-if-changed=bytecodes.toml");
}

macro_rules! w {
    ($self:ident, $($arg:tt)*) => {
        writeln!($self.w, $($arg)*)
    };
}

struct BytecodeWriter<W> {
    w: W,
    toml: HashMap<String, BytecodeConfig>,
}
impl<W: io::Write> BytecodeWriter<W> {
    fn write(&mut self) -> io::Result<()> {
        self.write_bytecode_enum()?;
        self.write_bytecode_impl()?;
        self.write_bytecode_display_impl()?;

        Ok(())
    }

    fn write_bytecode_enum(&mut self) -> io::Result<()> {
        w!(self, "#[allow(non_camel_case_types)]")?;
        w!(self, "pub enum Bytecode {{")?;
        self.write_bytecode_enum_variants()?;
        w!(self, "}}")?;

        Ok(())
    }

    fn write_bytecode_impl(&mut self) -> io::Result<()> {
        macro_rules! ws {
            ($($arg:tt)*) => {
                w!(self, $($arg)*)
            };
        }

        ws!("impl Bytecode {{")?;
        ws!("  pub fn read<R: crate::BytecodeStream>(r: &mut R) -> crate::Result<Self> {{")?;
        ws!("    match r.read_u8()? {{")?;
        self.write_bytecode_enum_matches()?;
        ws!("      b => Err(crate::Error::UnknownBytecode(b)),")?;
        ws!("    }}")?;
        ws!("  }}")?;
        ws!("}}")?;

        Ok(())
    }

    fn write_bytecode_enum_variants(&mut self) -> io::Result<()> {
        for (key, BytecodeConfig { format, .. }) in &self.toml {
            let param = match format {
                Some(BytecodeFormat::Wide) => "(u16)",
                Some(BytecodeFormat::Byte) => "(u8)",
                None => "",
            };

            w!(self, "  r#{key}{param},")?;
        }

        Ok(())
    }

    fn write_bytecode_enum_matches(&mut self) -> io::Result<()> {
        for (key, BytecodeConfig { opcode, format }) in &self.toml {
            w!(self, "      {opcode} => {{")?;
            let arg = match format {
                Some(BytecodeFormat::Wide) => {
                    w!(self, "        let indexbyte1 = r.read_u8()? as u16;")?;
                    w!(self, "        let indexbyte2 = r.read_u8()? as u16;")?;
                    "((indexbyte1 << 8) | indexbyte2)"
                }
                Some(BytecodeFormat::Byte) => {
                    w!(self, "        let index = r.read_u8()?;")?;
                    "(index)"
                }
                None => "",
            };
            w!(self, "        Ok(Bytecode::r#{key}{arg})")?;
            w!(self, "      }},")?;
        }

        Ok(())
    }

    fn write_bytecode_display_impl(&mut self) -> io::Result<()> {
        macro_rules! ws {
            ($($arg:tt)*) => {
                w!(self, $($arg)*)
            };
        }

        ws!("impl std::fmt::Display for Bytecode {{")?;
        ws!("  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {{")?;
        ws!("    match self {{")?;
        for (key, BytecodeConfig { format, .. }) in &self.toml {
            match format {
                Some(BytecodeFormat::Wide | BytecodeFormat::Byte) => {
                    ws!(
                        "      Bytecode::r#{key}(n) => write!(f, \"{{:<13}} #{{n}}\", \"{key}\")?,"
                    )?;
                }
                None => {
                    ws!("      Bytecode::r#{key} => write!(f, \"{key}\")?,")?;
                }
            }
        }
        ws!("    }}")?;
        ws!("    Ok(())")?;
        ws!("  }}")?;
        ws!("}}")?;

        Ok(())
    }
}
