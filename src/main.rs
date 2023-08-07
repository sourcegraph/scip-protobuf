use clap::Parser;
use std::{
    collections::HashMap,
    fs::{self, canonicalize, File},
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    process, vec,
};

use protobuf::{
    descriptor::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FileDescriptorProto, FileDescriptorSet,
    },
    Enum, Message, MessageField,
};
use scip::{
    symbol::format_symbol,
    types::{
        descriptor::Suffix, Descriptor, Document, Index, Metadata, Occurrence, Package, Symbol,
        SymbolInformation, SymbolRole, TextEncoding, ToolInfo,
    },
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'i', long = "in")]
    input: PathBuf,

    #[arg(short = 'o', long = "out")]
    output: PathBuf,

    #[arg(short = 'r', long = "root")]
    root: PathBuf,
}

type TypeTree<'a> = HashMap<String, TypeTreeValue<'a>>;

#[derive(Debug)]
struct TypeTreeValue<'a> {
    name: String,
    data: TypeTreeData<'a>,
    children: TypeTree<'a>,
}

#[derive(Debug)]
enum TypeTreeData<'a> {
    Package(()),
    MessageDescriptor(&'a DescriptorProto),
    EnumDescriptor(&'a EnumDescriptorProto),
}

enum DescriptorPathSegment<'a> {
    FileDescriptor(&'a FileDescriptorProto),
    MessageDescriptors(&'a Vec<DescriptorProto>),
    MessageDescriptor(&'a DescriptorProto),
    EnumDescriptors(&'a Vec<EnumDescriptorProto>),
    EnumDescriptor(&'a EnumDescriptorProto),

    FieldDescriptors(&'a Vec<FieldDescriptorProto>),
    FieldDescriptor(&'a FieldDescriptorProto),
    EnumValueDescriptors(&'a Vec<EnumValueDescriptorProto>),
    EnumValueDescriptor(&'a EnumValueDescriptorProto),
}

enum DescriptorsFromPathResult {
    Descriptors(Vec<Descriptor>),
    TypeToResolve(String),
}

fn get_descriptors_from_path(
    start: DescriptorPathSegment,
    path: &Vec<i32>,
) -> Option<DescriptorsFromPathResult> {
    let mut current_segment = start;
    let mut descriptors = vec![];

    // TODO: Implement all paths

    for (i, field) in path.iter().enumerate() {
        match current_segment {
            DescriptorPathSegment::FileDescriptor(fd) => match field {
                4 => current_segment = DescriptorPathSegment::MessageDescriptors(&fd.message_type),
                5 => current_segment = DescriptorPathSegment::EnumDescriptors(&fd.enum_type),

                _ => return None,
            },
            DescriptorPathSegment::MessageDescriptors(msgs) => {
                current_segment = DescriptorPathSegment::MessageDescriptor(&msgs[*field as usize]);
                if i == path.len() - 1 {
                    return None;
                }
            }
            DescriptorPathSegment::MessageDescriptor(msg) => {
                descriptors.push(Descriptor {
                    name: msg.name.clone().unwrap(),
                    suffix: Suffix::Type.into(),
                    ..Default::default()
                });

                match field {
                    1 => {
                        // name, see above
                    }
                    2 => current_segment = DescriptorPathSegment::FieldDescriptors(&msg.field),
                    3 => {
                        current_segment =
                            DescriptorPathSegment::MessageDescriptors(&msg.nested_type)
                    }
                    4 => current_segment = DescriptorPathSegment::EnumDescriptors(&msg.enum_type),
                    _ => return None,
                }
            }
            DescriptorPathSegment::EnumDescriptors(enums) => {
                current_segment = DescriptorPathSegment::EnumDescriptor(&enums[*field as usize]);
                if i == path.len() - 1 {
                    return None;
                }
            }
            DescriptorPathSegment::EnumDescriptor(en) => match field {
                1 => {
                    descriptors.push(Descriptor {
                        name: en.name.clone().unwrap(),
                        suffix: Suffix::Type.into(),
                        ..Default::default()
                    });
                }
                2 => current_segment = DescriptorPathSegment::EnumValueDescriptors(&en.value),
                _ => return None,
            },

            DescriptorPathSegment::FieldDescriptors(fields) => {
                current_segment = DescriptorPathSegment::FieldDescriptor(&fields[*field as usize]);
                if i == path.len() - 1 {
                    return None;
                }
            }
            DescriptorPathSegment::FieldDescriptor(field_info) => match field {
                1 => {
                    descriptors.push(Descriptor {
                        name: field_info.name.clone().unwrap(),
                        suffix: Suffix::Term.into(),
                        ..Default::default()
                    });
                }
                6 => {
                    // Names always seem to be .a.b.c (check this assumption)
                    return Some(DescriptorsFromPathResult::TypeToResolve(
                        field_info.type_name.clone().unwrap(),
                    ));
                }
                _ => return None,
            },
            DescriptorPathSegment::EnumValueDescriptors(values) => {
                current_segment =
                    DescriptorPathSegment::EnumValueDescriptor(&values[*field as usize]);
                if i == path.len() - 1 {
                    return None;
                }
            }
            DescriptorPathSegment::EnumValueDescriptor(value) => match field {
                1 => {
                    descriptors.push(Descriptor {
                        name: value.name.clone().unwrap(),
                        suffix: Suffix::Term.into(),
                        ..Default::default()
                    });
                }
                _ => return None,
            },
        }
    }

    Some(DescriptorsFromPathResult::Descriptors(descriptors))
}

// TODO: DOD-ify
fn populate_type_tree<'a>(tree: &mut TypeTree<'a>, file: &'a FileDescriptorProto) {
    let pkg = file.package.clone().unwrap();
    if !tree.contains_key(&pkg) {
        tree.insert(
            pkg.clone(),
            TypeTreeValue {
                name: pkg.clone(),
                data: TypeTreeData::Package(()),
                children: TypeTree::new(),
            },
        );
    }

    let package = tree.get_mut(&pkg).unwrap();

    for msg in &file.message_type {
        package.children.insert(
            msg.name.clone().unwrap(),
            TypeTreeValue {
                name: msg.name.clone().unwrap(),
                data: TypeTreeData::MessageDescriptor(msg),
                children: TypeTree::new(),
            },
        );
        populate_type_tree_internal(
            package
                .children
                .get_mut(msg.name.as_ref().unwrap())
                .unwrap(),
        );
    }

    for en in &file.enum_type {
        package.children.insert(
            en.name.clone().unwrap(),
            TypeTreeValue {
                name: en.name.clone().unwrap(),
                data: TypeTreeData::EnumDescriptor(en),
                children: TypeTree::new(),
            },
        );
        populate_type_tree_internal(package.children.get_mut(en.name.as_ref().unwrap()).unwrap());
    }
}

// TODO: DOD-ify
fn populate_type_tree_internal(value: &mut TypeTreeValue) {
    match value.data {
        TypeTreeData::Package(_) => unreachable!(),
        TypeTreeData::MessageDescriptor(msg) => {
            for msg in &msg.nested_type {
                value.children.insert(
                    msg.name.clone().unwrap(),
                    TypeTreeValue {
                        name: msg.name.clone().unwrap(),

                        data: TypeTreeData::MessageDescriptor(msg),
                        children: TypeTree::new(),
                    },
                );
                populate_type_tree_internal(
                    value.children.get_mut(msg.name.as_ref().unwrap()).unwrap(),
                );
            }

            for en in &msg.enum_type {
                value.children.insert(
                    en.name.clone().unwrap(),
                    TypeTreeValue {
                        name: en.name.clone().unwrap(),

                        data: TypeTreeData::EnumDescriptor(en),
                        children: TypeTree::new(),
                    },
                );
                populate_type_tree_internal(
                    value.children.get_mut(en.name.as_ref().unwrap()).unwrap(),
                );
            }
        }
        TypeTreeData::EnumDescriptor(_) => {}
    }
}

fn type_tree_value_to_descriptor(value: &TypeTreeValue) -> Descriptor {
    Descriptor {
        name: value.name.clone(),
        suffix: match value.data {
            TypeTreeData::Package(_) => unreachable!(),
            TypeTreeData::MessageDescriptor(_) => Suffix::Type,
            TypeTreeData::EnumDescriptor(_) => Suffix::Type,
        }
        .into(),
        ..Default::default()
    }
}

fn main() {
    let args = Args::parse();

    let dset_file = match File::open(args.input) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open input file: {}", e);
            process::exit(1)
        }
    };

    let scip_file = match File::create(&args.output) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to create output file: {}", e);
            process::exit(1)
        }
    };

    let mut buf_reader = BufReader::new(dset_file);
    let mut buf_writer = BufWriter::new(scip_file);

    let request = FileDescriptorSet::parse_from_reader(&mut buf_reader).unwrap();

    let mut type_tree = TypeTree::new();

    for req in &request.file {
        populate_type_tree(&mut type_tree, req);
    }

    let mut documents = vec![];

    for req in &request.file {
        let mut symbols = vec![];
        let mut occurrences = vec![];

        for loc in &req.source_code_info.location {
            if let Some(result) =
                get_descriptors_from_path(DescriptorPathSegment::FileDescriptor(&req), &loc.path)
            {
                let mut is_definition = false;
                let package_name;
                let mut descriptors;

                match result {
                    DescriptorsFromPathResult::Descriptors(desc) => {
                        package_name = req.package.clone().or(Some(".".to_string())).unwrap();
                        descriptors = desc;
                        is_definition = true;
                    }
                    DescriptorsFromPathResult::TypeToResolve(ttr) => {
                        descriptors = vec![];

                        let mut seq = ttr.split(".").skip(1).collect::<Vec<&str>>();
                        seq.reverse();
                        let mut value = type_tree.get(&seq.pop().unwrap().to_string()).unwrap();
                        package_name = value.name.clone();

                        while let Some(key) = seq.pop() {
                            value = value.children.get(&key.to_string()).unwrap();
                            descriptors.push(type_tree_value_to_descriptor(value));
                        }
                    }
                }

                let symbol = format_symbol(Symbol {
                    scheme: ".".to_string(),
                    package: MessageField::some(Package {
                        manager: ".".to_string(),
                        name: package_name,
                        version: ".".to_string(),
                        ..Default::default()
                    }),
                    descriptors,
                    ..Default::default()
                });

                if is_definition {
                    symbols.push(SymbolInformation {
                        symbol: symbol.clone(),
                        ..Default::default()
                    });
                }

                occurrences.push(Occurrence {
                    symbol,
                    symbol_roles: if is_definition {
                        SymbolRole::Definition.value()
                    } else {
                        0
                    },
                    range: loc.span.clone(),
                    ..Default::default()
                })
            }
        }

        documents.push(Document {
            relative_path: req.name.clone().expect("Missing relative path (why?)"),
            // TODO: Add to scip spec language list
            language: "ProtocolBuffers".to_string(),
            symbols,
            occurrences,
            ..Default::default()
        });
    }

    let index = Index {
        metadata: MessageField::some(Metadata {
            project_root: "file://".to_string()
                + canonicalize(&args.root).unwrap().to_str().unwrap(),
            text_document_encoding: TextEncoding::UTF8.into(),
            tool_info: MessageField::some(ToolInfo {
                name: "scip-protobuf".to_string(),
                version: "0.0.1".to_string(),
                arguments: std::env::args().collect(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        documents,
        ..Default::default()
    };

    let file = fs::File::create(&args.output).expect("Could not open output file!");
    let mut file_buf_writer = BufWriter::new(file);
    index.write_to_writer(&mut file_buf_writer).unwrap();

    file_buf_writer.flush().unwrap();
    buf_writer.flush().unwrap();
}
