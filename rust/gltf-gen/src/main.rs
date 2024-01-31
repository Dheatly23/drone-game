//#![allow(dead_code)]

mod gltf;
mod indexes;
mod meshgen;
mod othergen;
mod parse;

use std::fs::{read_to_string, File, OpenOptions};
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Write};
use std::mem;
use std::path::PathBuf;

use anyhow::{bail, Error};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about)]
/// Generates GLTF file from description/configuration
struct Cli {
    #[arg(short, long)]
    dir: bool,

    #[arg(short, long)]
    /// Output file path
    output: Option<PathBuf>,

    /// Input configuration file
    file: PathBuf,

    /// Copyright of the asset
    #[arg(long)]
    copyright: Option<PathBuf>,
}

fn main() -> Result<(), Error> {
    let mut cli = Cli::parse();

    if cli.dir {
        let parent = cli.output.as_ref().unwrap_or(&cli.file);
        for d in cli.file.read_dir()? {
            let input = d?.path();
            if input.extension() != Some("json".as_ref()) || input.metadata()?.is_dir() {
                continue;
            }
            let mut output = parent.join(match input.file_name() {
                Some(v) => v,
                None => bail!("Path {} has no file name", input.to_string_lossy()),
            });
            output.set_extension("glb");
            process_file(input, output, &cli)?;
        }
    } else {
        let output = cli
            .output
            .take()
            .unwrap_or_else(|| cli.file.with_extension("glb"));
        process_file(mem::take(&mut cli.file), output, &cli)?;
    }

    Ok(())
}

fn process_file(input: PathBuf, output: PathBuf, cli: &Cli) -> Result<(), Error> {
    println!("Reading {}", input.to_string_lossy());
    let mut data: parse::Data =
        serde_json::from_reader(BufReader::with_capacity(4096, File::open(&input)?))?;
    data.filepath = input;
    for (name, i) in &mut data.animations {
        for (i, v) in i.keyframe.iter().enumerate() {
            if !v.time.is_finite() {
                bail!("Error at animation {name} keyframe index {i}: Time is not a number!")
            } else if v.time < 0.0 {
                bail!("Error at animation {name} keyframe index {i}: Time is negative!")
            }
        }
        i.orderize();
    }

    let mut index = indexes::Index::default();
    let mut gltf = gltf::Gltf::default();
    let mut buffer = Vec::new();

    gltf.asset.version = "2.0".to_owned();
    if let Some(path) = &cli.copyright {
        gltf.asset.copyright = read_to_string(path)?;
    }

    othergen::add_node(&data, &data.root_node, &mut gltf, &mut buffer, &mut index)?;
    othergen::bind_skins(&data, &mut gltf, &mut buffer, &mut index)?;
    for (k, v) in &data.animations {
        othergen::add_animation(v, k, &mut gltf, &mut buffer, &index)?;
    }

    gltf.scene = 0;
    gltf.scenes.push(gltf::Scene {
        nodes: vec![gltf.nodes.len() - 1],
    });
    gltf.buffers.push(gltf::Buffer {
        byte_length: buffer.len(),
    });

    println!("Writing {}", output.to_string_lossy());
    let output = OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(true)
        .create(true)
        .open(output)?;
    let mut output = BufWriter::with_capacity(4096, output);
    output.write_all(b"glTF\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00JSON")?;

    {
        let start = output.stream_position()?;
        serde_json::to_writer(&mut output, &gltf)?;
        let end = output.stream_position()?;
        output.seek(SeekFrom::Start(12))?;
        output.write_all(&((end - start) as u32).to_le_bytes())?;
        output.seek(SeekFrom::End(0))?;
    }

    output.write_all(&(buffer.len() as u32).to_le_bytes())?;
    output.write_all(b"BIN\x00")?;
    output.write_all(&buffer)?;

    let length = output.stream_position()? - 12;
    output.seek(SeekFrom::Start(8))?;
    output.write_all(&(length as u32).to_le_bytes())?;
    output.flush()?;
    drop(output);

    Ok(())
}
