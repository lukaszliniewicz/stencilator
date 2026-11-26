//! 3MF files: a zip holding an XML model, which every current slicer opens.
//!
//! Each object is a closed mesh with a name and a display colour (a base material, from
//! the core specification, so no extension is needed); the build places each one. Lengths
//! are written in millimetres as exact decimals of whole micrometres, so the same meshes
//! always give the same bytes.

use std::fmt::Write as _;

use crate::colour::Rgb;
use crate::mesh::Mesh;
use crate::zip::zip;
use crate::{Error, VERSION};

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
 <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
 <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>
"#;

const RELATIONSHIPS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
 <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>
"#;

/// One object of a 3MF file.
#[derive(Debug, Clone, Copy)]
pub struct Object<'a> {
    /// What slicers call it.
    pub name: &'a str,
    /// The colour slicers show it in.
    pub colour: Rgb,
    /// The solid, in micrometres.
    pub mesh: &'a Mesh,
    /// Where its origin goes on the build plate, in micrometres.
    pub position_um: (i32, i32),
}

/// A 3MF file holding the objects, in the order given.
///
/// # Errors
///
/// [`Error::Export`] when there are no objects, or the file would reach 4 GB.
pub fn write_3mf(objects: &[Object<'_>]) -> Result<Vec<u8>, Error> {
    if objects.is_empty() {
        return Err(Error::Export("There is nothing to put in the 3MF file.".to_owned()));
    }
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str("\n<model unit=\"millimeter\" xml:lang=\"en-GB\" ");
    xml.push_str("xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n");
    let _ = writeln!(xml, " <metadata name=\"Application\">Stencilator {VERSION}</metadata>");
    xml.push_str(" <resources>\n  <basematerials id=\"1\">\n");
    for object in objects {
        let [r, g, b] = object.colour;
        let _ = writeln!(xml, "   <base name=\"{}\" displaycolor=\"#{r:02X}{g:02X}{b:02X}\"/>", escape(object.name));
    }
    xml.push_str("  </basematerials>\n");
    for (index, object) in objects.iter().enumerate() {
        let _ = writeln!(
            xml,
            "  <object id=\"{}\" type=\"model\" name=\"{}\" pid=\"1\" pindex=\"{index}\">\n   <mesh>\n    <vertices>",
            index + 2,
            escape(object.name)
        );
        for &[x, y, z] in &object.mesh.vertices {
            let _ = writeln!(xml, "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", Mm(x), Mm(y), Mm(z));
        }
        xml.push_str("    </vertices>\n    <triangles>\n");
        for [a, b, c] in &object.mesh.triangles {
            let _ = writeln!(xml, "     <triangle v1=\"{a}\" v2=\"{b}\" v3=\"{c}\"/>");
        }
        xml.push_str("    </triangles>\n   </mesh>\n  </object>\n");
    }
    xml.push_str(" </resources>\n <build>\n");
    for (index, object) in objects.iter().enumerate() {
        let (x, y) = object.position_um;
        let _ =
            writeln!(xml, "  <item objectid=\"{}\" transform=\"1 0 0 0 1 0 0 0 1 {} {} 0\"/>", index + 2, Mm(x), Mm(y));
    }
    xml.push_str(" </build>\n</model>\n");
    zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELATIONSHIPS.as_bytes()),
        ("3D/3dmodel.model", xml.as_bytes()),
    ])
}

/// Whole micrometres written as millimetres, exactly and without trailing zeros.
struct Mm(i32);

impl std::fmt::Display for Mm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let um = self.0.unsigned_abs();
        let sign = if self.0 < 0 { "-" } else { "" };
        let (whole, part) = (um / 1000, um % 1000);
        if part == 0 {
            write!(f, "{sign}{whole}")
        } else {
            let digits = format!("{part:03}");
            write!(f, "{sign}{whole}.{}", digits.trim_end_matches('0'))
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit cube, 1 mm a side, counter-clockwise from outside.
    fn cube() -> Mesh {
        let vertices = (0..8).map(|i| [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|c| c * 1000)).collect();
        let triangles = vec![
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ];
        Mesh { vertices, triangles }
    }

    #[test]
    fn millimetres_are_exact() {
        let text = |um| Mm(um).to_string();
        assert_eq!([text(0), text(1200), text(1), text(-1500), text(250_010)], ["0", "1.2", "0.001", "-1.5", "250.01"]);
    }

    #[test]
    fn objects_are_named_coloured_and_placed() {
        let cube = cube();
        let objects = [
            Object { name: "plate-01-1d2f5c", colour: [0x1d, 0x2f, 0x5c], mesh: &cube, position_um: (0, 0) },
            Object { name: "a <b> & \"c\"", colour: [255, 255, 255], mesh: &cube, position_um: (12_500, -3000) },
        ];
        let file = write_3mf(&objects).unwrap();
        assert_eq!(file, write_3mf(&objects).unwrap(), "the same bytes every time");
        let entries = crate::zip::unzip(&file);
        let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
        assert_eq!(names, ["[Content_Types].xml", "_rels/.rels", "3D/3dmodel.model"]);
        let xml = String::from_utf8(entries[2].1.clone()).unwrap();
        assert!(xml.contains(r##"<base name="plate-01-1d2f5c" displaycolor="#1D2F5C"/>"##), "{xml}");
        assert!(xml.contains(r#"name="a &lt;b&gt; &amp; &quot;c&quot;" pid="1" pindex="1""#), "{xml}");
        assert!(xml.contains(r#"<vertex x="1" y="1" z="1"/>"#), "{xml}");
        assert!(xml.contains(r#"<item objectid="3" transform="1 0 0 0 1 0 0 0 1 12.5 -3 0"/>"#), "{xml}");
        assert_eq!(xml.matches("<triangle ").count(), 24);
        assert_eq!(write_3mf(&[]).unwrap_err().to_string(), "There is nothing to put in the 3MF file.");
    }
}
