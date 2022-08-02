use clap::Parser;
use std::iter::Iterator;
use std::thread;

lazy_static::lazy_static! {
static ref UMI_PATTERN: regex::Regex = regex::Regex::new("^(N{2,})([ATCG]*)$").unwrap();
}
struct Nucleotide {
    offset: usize,
    spacer: String,
}
enum ExtractedRecord {
    Empty,
    Valid {
        read: bio::io::fastq::Record,
        umi: Vec<u8>,
    },
}
fn read_fastq(
    path: &std::string::String,
) -> bio::io::fastq::Reader<std::io::BufReader<std::fs::File>> {
    std::fs::File::open(path)
        .map(bio::io::fastq::Reader::new)
        .unwrap()
}
fn output_file(name: &str) -> bio::io::fastq::Writer<std::fs::File> {
    std::fs::File::create(format!("{}.fastq", name))
        .map(bio::io::fastq::Writer::new)
        .unwrap()
}

#[derive(clap::Parser)]
struct Opts {
    #[clap(long, default_value = "integrated")]
    prefix: String,
    #[clap(long, required = true)]
    r1_in: Vec<String>,
    #[clap(long)]
    r2_in: Vec<String>,
    #[clap(subcommand)]
    sub: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    #[clap(name = "separate")]
    Separate {
        #[clap(long, required = true)]
        ru_in: Vec<String>,
    },
    #[clap(name = "inline")]
    Inline {
        #[clap(long, required = true)]
        pattern1: String,
        #[clap(long)]
        pattern2: Option<String>,
    },
}

fn write_to_file(
    input: bio::io::fastq::Record,
    mut output: bio::io::fastq::Writer<std::fs::File>,
    umi: &[u8],
    second: bool,
) -> bio::io::fastq::Writer<std::fs::File> {
    let s = input;
    if second {
        let header = &[s.id(), ":", std::str::from_utf8(&umi).unwrap()].concat();
        let mut string = String::from(s.desc().unwrap());
        string.replace_range(0..1, "2");
        let desc: Option<&str> = Some(&string);
        output.write(&header, desc, s.seq(), s.qual()).unwrap();
    } else {
        let header = &[s.id(), ":", std::str::from_utf8(&umi).unwrap()].concat();
        output.write(&header, s.desc(), s.seq(), s.qual()).unwrap();
    }
    output
}
fn parse(pattern: &str) -> Option<Nucleotide> {
    if let Some(captures) = UMI_PATTERN.captures(pattern) {
        Some(Nucleotide {
            offset: captures.get(1)?.end(),
            spacer: captures.get(2)?.as_str().into(),
        })
    } else {
        panic!("")
    }
}
fn extract(record: bio::io::fastq::Record, pattern: &str) -> ExtractedRecord {
    let handler = parse(pattern);
    match handler {
        Some(Nucleotide { offset, spacer }) => {
            let end = offset + spacer.len();
            if end <= record.seq().len() && record.seq()[offset..end] == *spacer.as_bytes() {
                let read = bio::io::fastq::Record::with_attrs(
                    record.id(),
                    record.desc(),
                    record.seq()[end..record.seq().len()].into(),
                    record.qual()[end..record.qual().len()].into(),
                );
                ExtractedRecord::Valid {
                    read: read,
                    umi: record.seq()[0..offset].into(),
                }
            } else {
                ExtractedRecord::Empty
            }
        }
        None => panic!(""),
    }
}
fn write_inline_to_file(
    record: ExtractedRecord,
    write_file: bio::io::fastq::Writer<std::fs::File>,
    second: bool,
) -> bio::io::fastq::Writer<std::fs::File> {
    match record {
        ExtractedRecord::Empty => panic!("Not Valid UMI/ Record"),
        ExtractedRecord::Valid { read, umi } => write_to_file(read, write_file, &umi, second),
    }
}
fn main() {
    let args = Opts::parse();

    // Create write files
    let mut write_file_r1 = output_file(&format!("{}1", &args.prefix));

    // read supplied files
    let r1 = read_fastq(&args.r1_in[0]).records();
    match args.sub {
        Commands::Separate { ru_in } => {
            let ru1 = ru_in.clone();
            let handle1 = thread::spawn(move || {
                let ru = read_fastq(&ru_in[0]).records();
                for (r1_rec, ru_rec) in r1.zip(ru) {
                    write_file_r1 =
                        write_to_file(r1_rec.unwrap(), write_file_r1, ru_rec.unwrap().seq(), false);
                }
            });
            let mut l = Vec::new();
            l.push(handle1);
            if !&args.r2_in.is_empty() {
                let r2 = read_fastq(&args.r2_in[0]).records();
                let mut write_file_r2 = output_file(&format!("{}2", &args.prefix));
                let handle2 = thread::spawn(move || {
                    let ru = read_fastq(&ru1[0]).records();
                    for (r2_rec, ru_rec) in r2.zip(ru) {
                        write_file_r2 = write_to_file(
                            r2_rec.unwrap(),
                            write_file_r2,
                            ru_rec.unwrap().seq(),
                            true,
                        );
                    }
                });
                l.push(handle2);
            }
            for i in l {
                if !i.is_finished() {
                    i.join().unwrap();
                }
            }
        }
        Commands::Inline { pattern1, pattern2 } => {
            let handle1 = thread::spawn(move || {
                for r1_rec in r1 {
                    let record1 = extract(r1_rec.unwrap(), &pattern1);
                    write_file_r1 = write_inline_to_file(record1, write_file_r1, false);
                }
            });
            let mut l = Vec::new();
            l.push(handle1);

            if !&args.r2_in.is_empty() {
                let mut write_file_r2 = output_file(&format!("{}2", &args.prefix));
                let r2 = read_fastq(&args.r2_in[0]).records();
                let handle2 = thread::spawn(move || {
                    for r2_rec in r2 {
                        let record2 = extract(r2_rec.unwrap(), &(pattern2.as_ref().unwrap()));
                        write_file_r2 = write_inline_to_file(record2, write_file_r2, true);
                    }
                });
                l.push(handle2);
            }
            for i in l {
                if !i.is_finished() {
                    i.join().unwrap();
                }
            }
        }
    }
}