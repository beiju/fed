#![feature(let_chains)]

use fed::FedEvent;
use itertools::Itertools;
use schemars::Schema;
use schemars::generate::SchemaSettings;
use schemars::transform::Transform;

#[derive(Debug, Clone)]
pub struct SurfaceEnumTitle;

impl Transform for SurfaceEnumTitle {
    fn transform(&mut self, schema: &mut Schema) {
        // this is good coding
        todo!("What is going on here")
        //     if let Some(subschemas) = &mut schema {
        //         if let Some(object) = &mut subschemas.one_of {
        //             for schema in object {
        //                 if let Object(obj) = schema {
        //                     let values = if let Some(values) = &mut obj.enum_values {
        //                         values
        //                     } else if let Some(properties) = &mut obj.object {
        //                         if let Some(type_prop) = properties.properties.get_mut("type") {
        //                             if let Object(obj) = type_prop {
        //                                 if let Some(values) = &mut obj.enum_values {
        //                                     values
        //                                 } else {
        //                                     continue;
        //                                 }
        //                             } else {
        //                                 continue;
        //                             }
        //                         } else if let Some((name, _)) = properties.properties.iter().exactly_one().ok() {
        //                             if let Some(metadata) = &mut obj.metadata {
        //                                 metadata.title.get_or_insert(name.to_string());
        //                             }
        //                             continue;
        //                         } else {
        //                             continue;
        //                         }
        //                     } else {
        //                         continue;
        //                     };
        //                     if let Some(first_value) = values.first() {
        //                         if let Some(name) = first_value.as_str() {
        //                             if let Some(metadata) = &mut obj.metadata {
        //                                 metadata.title.get_or_insert(name.to_string());
        //                             }
        //                         }
        //                     }
        //                 }
        //             }
        //         }
        //     }
        //
        //     // Then delegate to default implementation to visit any subschemas
        //     visit_schema_object(self, schema);
    }
}

fn main() {
    let schema = SchemaSettings::default()
        .with_transform(SurfaceEnumTitle)
        .into_generator()
        .into_root_schema_for::<FedEvent>();

    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
}
