use rustc_hash::FxHashMap;

use rbx_reflection::{
    ClassTag, DataType, PropertyKind as RbxPropertyKind, PropertySerialization, PropertyTag,
    ReflectionDatabase, Scriptability as RbxScriptability,
};
use rbx_types::{Variant, VariantType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PropertyId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnumId(pub u16);

/// Class tags from the API dump.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClassFlags {
    pub deprecated: bool,
    pub not_browsable: bool,
    pub not_creatable: bool,
    pub not_replicated: bool,
    pub player_replicated: bool,
    pub service: bool,
    pub settings: bool,
    pub user_settings: bool,
}

/// Property tags from the API dump.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PropertyFlags {
    pub deprecated: bool,
    pub hidden: bool,
    pub not_browsable: bool,
    pub not_replicated: bool,
    pub not_scriptable: bool,
    pub read_only: bool,
    pub write_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scriptability {
    None,
    ReadWrite,
    Read,
    Write,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyType {
    Value(VariantType),
    Enum(EnumId),
}

/// How a canonical property is stored in files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Serialization {
    Serializes,
    DoesNotSerialize,
    /// Stored in files under a different (often legacy) name.
    SerializesAs(&'static str),
    /// A legacy property migrated to newer properties when loading.
    Migrate {
        to: Vec<&'static str>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyKind {
    Canonical(Serialization),
    /// Another name for a canonical property on the same class or an ancestor.
    Alias {
        canonical: &'static str,
    },
}

#[derive(Debug)]
pub struct Property {
    pub id: PropertyId,
    pub name: &'static str,
    pub owner: ClassId,
    pub ty: PropertyType,
    pub scriptability: Scriptability,
    pub flags: PropertyFlags,
    pub kind: PropertyKind,
}

#[derive(Debug)]
pub struct Class {
    pub id: ClassId,
    pub name: &'static str,
    pub superclass: Option<ClassId>,
    pub flags: ClassFlags,
    /// Properties declared on this class only.
    pub own_properties: Vec<PropertyId>,
    /// All properties, root ancestor's first, then this class's own.
    pub all_properties: Vec<PropertyId>,
    lookup: FxHashMap<&'static str, PropertyId>,
    defaults: FxHashMap<PropertyId, &'static Variant>,
}

#[derive(Debug)]
pub struct EnumInfo {
    pub id: EnumId,
    pub name: &'static str,
    /// Items sorted by value, then name.
    pub items: Vec<(&'static str, u32)>,
}

/// The compiled database. Build once via [`crate::database`].
#[derive(Debug)]
pub struct Database {
    pub version: [u32; 4],
    classes: Vec<Class>,
    properties: Vec<Property>,
    enums: Vec<EnumInfo>,
    class_by_name: FxHashMap<&'static str, ClassId>,
    enum_by_name: FxHashMap<&'static str, EnumId>,
}

impl Database {
    pub(crate) fn build(source: &'static ReflectionDatabase<'static>) -> Self {
        // Enums first, sorted by name for stable IDs.
        let mut enum_names: Vec<&'static str> = source.enums.keys().copied().collect();
        enum_names.sort_unstable();
        let mut enums = Vec::with_capacity(enum_names.len());
        let mut enum_by_name =
            FxHashMap::with_capacity_and_hasher(enum_names.len(), Default::default());
        for (i, name) in enum_names.iter().enumerate() {
            let id = EnumId(u16::try_from(i).expect("too many enums"));
            let mut items: Vec<(&'static str, u32)> = source.enums[name]
                .items
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect();
            items.sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0)));
            enum_by_name.insert(*name, id);
            enums.push(EnumInfo { id, name, items });
        }

        // Classes sorted by name for stable IDs.
        let mut class_names: Vec<&'static str> = source.classes.keys().copied().collect();
        class_names.sort_unstable();
        let class_by_name: FxHashMap<&'static str, ClassId> = class_names
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, ClassId(u16::try_from(i).expect("too many classes"))))
            .collect();

        let mut classes = Vec::with_capacity(class_names.len());
        let mut properties = Vec::new();
        for name in &class_names {
            let desc = &source.classes[name];
            let id = class_by_name[name];
            let mut prop_names: Vec<&'static str> = desc.properties.keys().copied().collect();
            prop_names.sort_unstable();
            let mut own_properties = Vec::with_capacity(prop_names.len());
            for prop_name in prop_names {
                let p = &desc.properties[prop_name];
                let pid = PropertyId(u32::try_from(properties.len()).expect("too many properties"));
                let ty = match &p.data_type {
                    DataType::Enum(e) => PropertyType::Enum(
                        *enum_by_name
                            .get(e)
                            .unwrap_or_else(|| panic!("unknown enum {e}")),
                    ),
                    DataType::Value(v) => PropertyType::Value(*v),
                    other => PropertyType::Value(other.ty()),
                };
                properties.push(Property {
                    id: pid,
                    name: prop_name,
                    owner: id,
                    ty,
                    scriptability: convert_scriptability(p.scriptability),
                    flags: property_flags(&p.tags),
                    kind: convert_kind(&p.kind),
                });
                own_properties.push(pid);
            }
            classes.push(Class {
                id,
                name,
                superclass: desc.superclass.map(|s| class_by_name[s]),
                flags: class_flags(&desc.tags),
                own_properties,
                all_properties: Vec::new(),
                lookup: FxHashMap::default(),
                defaults: FxHashMap::default(),
            });
        }

        // Inherited property lists and lookups.
        for i in 0..classes.len() {
            let mut chain = Vec::new();
            let mut cur = Some(classes[i].id);
            while let Some(c) = cur {
                chain.push(c);
                cur = classes[usize::from(c.0)].superclass;
            }
            let mut all = Vec::new();
            let mut lookup = FxHashMap::default();
            for c in chain.iter().rev() {
                for &pid in &classes[usize::from(c.0)].own_properties {
                    all.push(pid);
                    lookup.insert(properties[pid.0 as usize].name, pid);
                }
            }
            // Defaults: the most derived class that states one wins.
            let mut defaults = FxHashMap::default();
            for &pid in &all {
                let pname = properties[pid.0 as usize].name;
                for c in &chain {
                    let cname = classes[usize::from(c.0)].name;
                    if let Some(v) = source.classes[cname].default_properties.get(pname) {
                        defaults.insert(pid, v);
                        break;
                    }
                }
            }
            classes[i].all_properties = all;
            classes[i].lookup = lookup;
            classes[i].defaults = defaults;
        }

        Self {
            version: source.version,
            classes,
            properties,
            enums,
            class_by_name,
            enum_by_name,
        }
    }

    pub fn classes(&self) -> &[Class] {
        &self.classes
    }

    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes[usize::from(id.0)]
    }

    pub fn class_by_name(&self, name: &str) -> Option<&Class> {
        self.class_by_name.get(name).map(|id| self.class(*id))
    }

    pub fn property(&self, id: PropertyId) -> &Property {
        &self.properties[id.0 as usize]
    }

    pub fn property_count(&self) -> usize {
        self.properties.len()
    }

    pub fn enums(&self) -> &[EnumInfo] {
        &self.enums
    }

    pub fn enum_info(&self, id: EnumId) -> &EnumInfo {
        &self.enums[usize::from(id.0)]
    }

    pub fn enum_by_name(&self, name: &str) -> Option<&EnumInfo> {
        self.enum_by_name.get(name).map(|id| self.enum_info(*id))
    }

    /// Find a property on a class or any ancestor, by its exact name.
    pub fn find_property(&self, class: ClassId, name: &str) -> Option<&Property> {
        self.class(class)
            .lookup
            .get(name)
            .map(|id| self.property(*id))
    }

    /// Like [`Database::find_property`], but follows aliases to the canonical property.
    pub fn find_canonical_property(&self, class: ClassId, name: &str) -> Option<&Property> {
        let prop = self.find_property(class, name)?;
        match &prop.kind {
            PropertyKind::Alias { canonical } => self.find_property(class, canonical),
            PropertyKind::Canonical(_) => Some(prop),
        }
    }

    /// The default value of a property for a concrete class, if the database has one.
    pub fn default_value(&self, class: ClassId, property: PropertyId) -> Option<&'static Variant> {
        self.class(class).defaults.get(&property).copied()
    }

    /// `Instance:IsA`: whether `class` is `ancestor` or inherits from it.
    pub fn is_a(&self, class: ClassId, ancestor: ClassId) -> bool {
        let mut cur = Some(class);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.class(c).superclass;
        }
        false
    }

    /// The class followed by each superclass up to `Instance`.
    pub fn superclasses(&self, class: ClassId) -> impl Iterator<Item = &Class> {
        let mut cur = Some(class);
        std::iter::from_fn(move || {
            let c = cur?;
            let class = self.class(c);
            cur = class.superclass;
            Some(class)
        })
    }
}

fn convert_scriptability(s: RbxScriptability) -> Scriptability {
    match s {
        RbxScriptability::ReadWrite => Scriptability::ReadWrite,
        RbxScriptability::Read => Scriptability::Read,
        RbxScriptability::Write => Scriptability::Write,
        RbxScriptability::Custom => Scriptability::Custom,
        _ => Scriptability::None,
    }
}

fn convert_kind(kind: &'static RbxPropertyKind<'static>) -> PropertyKind {
    match kind {
        RbxPropertyKind::Alias { alias_for } => PropertyKind::Alias {
            canonical: alias_for,
        },
        RbxPropertyKind::Canonical { serialization } => {
            PropertyKind::Canonical(match serialization {
                PropertySerialization::Serializes => Serialization::Serializes,
                PropertySerialization::SerializesAs(name) => Serialization::SerializesAs(name),
                PropertySerialization::Migrate(m) => Serialization::Migrate {
                    to: m.new_property_names().to_vec(),
                },
                _ => Serialization::DoesNotSerialize,
            })
        }
        _ => PropertyKind::Canonical(Serialization::DoesNotSerialize),
    }
}

fn class_flags(tags: &std::collections::HashSet<ClassTag>) -> ClassFlags {
    ClassFlags {
        deprecated: tags.contains(&ClassTag::Deprecated),
        not_browsable: tags.contains(&ClassTag::NotBrowsable),
        not_creatable: tags.contains(&ClassTag::NotCreatable),
        not_replicated: tags.contains(&ClassTag::NotReplicated),
        player_replicated: tags.contains(&ClassTag::PlayerReplicated),
        service: tags.contains(&ClassTag::Service),
        settings: tags.contains(&ClassTag::Settings),
        user_settings: tags.contains(&ClassTag::UserSettings),
    }
}

fn property_flags(tags: &std::collections::HashSet<PropertyTag>) -> PropertyFlags {
    PropertyFlags {
        deprecated: tags.contains(&PropertyTag::Deprecated),
        hidden: tags.contains(&PropertyTag::Hidden),
        not_browsable: tags.contains(&PropertyTag::NotBrowsable),
        not_replicated: tags.contains(&PropertyTag::NotReplicated),
        not_scriptable: tags.contains(&PropertyTag::NotScriptable),
        read_only: tags.contains(&PropertyTag::ReadOnly),
        write_only: tags.contains(&PropertyTag::WriteOnly),
    }
}

#[cfg(test)]
mod tests {
    use crate::database;

    #[test]
    fn builds_with_expected_scale() {
        let db = database();
        assert!(
            db.classes().len() > 500,
            "only {} classes",
            db.classes().len()
        );
        assert!(db.enums().len() > 300);
    }

    #[test]
    fn part_inherits_from_instance() {
        let db = database();
        let part = db.class_by_name("Part").unwrap().id;
        let base_part = db.class_by_name("BasePart").unwrap().id;
        let instance = db.class_by_name("Instance").unwrap().id;
        assert!(db.is_a(part, base_part));
        assert!(db.is_a(part, instance));
        assert!(!db.is_a(base_part, part));
        let chain: Vec<_> = db.superclasses(part).map(|c| c.name).collect();
        assert_eq!(chain.first(), Some(&"Part"));
        // Modern dumps place `Object` above `Instance`.
        assert_eq!(&chain[chain.len() - 2..], &["Instance", "Object"]);
    }

    #[test]
    fn inherited_properties_resolve() {
        let db = database();
        let part = db.class_by_name("Part").unwrap().id;
        assert_eq!(db.find_property(part, "Anchored").unwrap().name, "Anchored");
        assert_eq!(
            db.find_property(part, "Archivable").unwrap().name,
            "Archivable"
        );
        assert!(db.find_property(part, "NotARealProperty").is_none());
    }

    #[test]
    fn part_size_alias_and_default() {
        let db = database();
        let part = db.class_by_name("Part").unwrap().id;
        // `size` is the legacy alias; files store `Size` under that name.
        let size = db.find_canonical_property(part, "size").unwrap();
        assert_eq!(size.name, "Size");
        assert_eq!(
            size.kind,
            crate::PropertyKind::Canonical(crate::Serialization::SerializesAs("size"))
        );
        // Instance.new("Part") is 4 x 1.2 x 2 (the legacy Brick height).
        let default = db
            .default_value(part, size.id)
            .expect("Part has a default size");
        assert_eq!(
            *default,
            rbx_types::Variant::Vector3(rbx_types::Vector3::new(4.0, 1.2, 2.0))
        );
    }

    #[test]
    fn enums_resolve() {
        let db = database();
        let material = db.enum_by_name("Material").unwrap();
        assert!(
            material
                .items
                .iter()
                .any(|(n, v)| *n == "Plastic" && *v == 256)
        );
    }

    #[test]
    fn deprecated_tags_present() {
        let db = database();
        let hopper_bin = db.class_by_name("HopperBin").unwrap();
        assert!(hopper_bin.flags.deprecated);
    }
}
