use std::{
    fs,
    io::{stdin, stdout, BufReader, BufWriter, Write},
};

use protobuf::{
    descriptor::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FileDescriptorProto,
    },
    plugin::{CodeGeneratorRequest, CodeGeneratorResponse},
    Message, MessageField,
};
use scip::{
    symbol::format_symbol,
    types::{
        descriptor::Suffix, Descriptor, Document, Index, Metadata, Occurrence, Package, Symbol,
        TextEncoding, ToolInfo,
    },
};

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

fn get_descriptors_from_path(
    start: DescriptorPathSegment,
    path: &Vec<i32>,
) -> Option<Vec<Descriptor>> {
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

    Some(descriptors)
}

fn main() {
    let mut buf_reader = BufReader::new(stdin());
    let mut buf_writer = BufWriter::new(stdout());

    let request = CodeGeneratorRequest::parse_from_reader(&mut buf_reader).unwrap();

    let params = request
        .parameter
        .expect("[root that matches proto path/is your scip root] [output path]");
    let arguments = params.split(" ").collect::<Vec<&str>>();

    if arguments.len() != 2 {
        panic!("[root that matches proto path/is your scip root] [output path]");
    }

    let root = arguments[0];
    let output_path = arguments[1];

    let mut documents = vec![];

    for req in request.proto_file {
        let mut occurrences = vec![];

        for loc in &req.source_code_info.location {
            if let Some(mut desc) =
                get_descriptors_from_path(DescriptorPathSegment::FileDescriptor(&req), &loc.path)
            {
                let mut descriptors = req
                    .name
                    .clone()
                    .expect("Missing relative path (why?)")
                    .split("/")
                    .map(|seg| Descriptor {
                        name: seg.to_string(),
                        suffix: Suffix::Namespace.into(),
                        ..Default::default()
                    })
                    .collect::<Vec<Descriptor>>();
                descriptors.append(&mut desc);
                occurrences.push(Occurrence {
                    symbol: format_symbol(Symbol {
                        scheme: ".".to_string(),
                        package: MessageField::some(Package {
                            manager: ".".to_string(),
                            name: req.package.clone().or(Some(".".to_string())).unwrap(),
                            version: ".".to_string(),
                            ..Default::default()
                        }),
                        descriptors,
                        ..Default::default()
                    }),
                    range: loc.span.clone(),
                    ..Default::default()
                })
            }
        }

        documents.push(Document {
            relative_path: req.name.expect("Missing relative path (why?)"),
            // TODO: Add to scip spec language list
            language: "ProtocolBuffers".to_string(),
            occurrences,
            ..Default::default()
        });
    }

    let index = Index {
        metadata: MessageField::some(Metadata {
            project_root: "file://".to_string() + root,
            text_document_encoding: TextEncoding::UTF8.into(),
            tool_info: MessageField::some(ToolInfo {
                name: "scip-protobuf".to_string(),
                version: "0.0.1".to_string(),
                arguments: arguments.iter().map(|v| v.to_string()).collect(),
                ..Default::default()
            }),
            ..Default::default()
        }),
        documents,
        ..Default::default()
    };

    // We send back no files as our data is not valid utf8 (it's binary)
    CodeGeneratorResponse {
        error: None,
        file: vec![],
        supported_features: Some(1),
        ..Default::default()
    }
    .write_to_writer(&mut buf_writer)
    .unwrap();

    let file = fs::File::create(output_path).expect("Could not open output file!");
    let mut file_buf_writer = BufWriter::new(file);
    index.write_to_writer(&mut file_buf_writer).unwrap();

    file_buf_writer.flush().unwrap();
    buf_writer.flush().unwrap();
}
