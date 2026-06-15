#[derive(Debug, PartialEq)]
pub struct TsonHeader {
    pub version: u8, // TSON version number
    pub blk_definition: u32, // Offset to the definition block
    pub blk_data: u32, // Offset to the data block
}

// implementations of TsonHeader:
impl TsonHeader {
    pub fn new(version: u8, blk_definition: u32, blk_data: u32) -> Self {
        Self {
            version,
            blk_definition,
            blk_data,
        }
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TsonError> {
        if bytes.len() < 9 {
            return Err(TsonError::ParseError("Header must be at least 9 bytes".to_string()));
        }
        let version = bytes[0];
        let blk_definition = u32::from_le_bytes(bytes[1..5].try_into().unwrap());
        let blk_data = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
        Ok(Self {
            version,
            blk_definition,
            blk_data,
        })
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(9);
        bytes.push(self.version);
        bytes.extend(&self.blk_definition.to_le_bytes());
        bytes.extend(&self.blk_data.to_le_bytes());
        bytes
    }
    pub fn validate(&self) -> Result<(), TsonError> {
        if self.version != 1 {
            return Err(TsonError::ParseError(format!("Unsupported TSON version: {}", self.version)));
        }
        Ok(())
    }
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
    pub fn set_blk_definition(&mut self, offset: u32) {
        self.blk_definition = offset;
    }
    pub fn set_blk_data(&mut self, offset: u32) {
        self.blk_data = offset;
    }
    pub fn display(&self) {
        println!("TSON Header:");
        println!("Version: {}", self.version);
        println!("Definition Block Offset: {}", self.blk_definition);
        println!("Data Block Offset: {}", self.blk_data);
    }
    pub fn from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, TsonError> {
        let mut header_bytes = [0u8; 9];
        reader.read_exact(&mut header_bytes)?;
        Self::from_bytes(&header_bytes)
    }
}

pub enum TsonData {
    Int(i32), // 32-bit signed integer
    Float(f32), // 32-bit floating-point number
    String(String), // UTF-8 encoded string
    Array(Vec<TsonData>), // Array of TsonData elements i.e normal values which are not compiled into the definition block
    Null, // Represents a null value
    Object,
}

#[derive(Debug, PartialEq)]
pub struct TsonDefinition {
    pub def_ends_at: u32, // Offset to the end of this definition in the data block
    pub def_type: TsonData, // Type of the definition
    pub index_name: i32, // This is used as the referencing index for the definition bloc.
    // Fields are empty for non-object definitions. For object definitions, this contains the field names and their corresponding definition indices or types.
    pub fields: Option<Vec<(String, TsonData)>>, // Vec of (field name, field type) pairs for object definitions
}

impl TsonDefinition {
    pub fn new(def_ends_at: u32, def_type: TsonData, index_name: i32, fields: Option<Vec<(String, TsonData)>>) -> Self {
        Self {
            def_ends_at,
            def_type,
            index_name,
            fields,
        }
    }
    pub fn add_field(&mut self, field_name: String, field_type: TsonData) {
        if let Some(fields) = &mut self.fields {
            fields.push((field_name, field_type));
        } else {
            self.fields = Some(vec![(field_name, field_type)]);
        }
    }
    pub fn get_field(&self, field_name: &str) -> Option<&TsonData> {
        if let Some(fields) = &self.fields {
            for (name, field_type) in fields {
                if name == field_name {
                    return Some(field_type);
                }
            }
        }
        None
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TsonError> {
        // Placeholder for actual implementation of parsing a TsonDefinition from bytes
        Ok(Self {
            def_ends_at: 0,
            def_type: TsonData::Null,
            index_name: 0,
            fields: None,
        })
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        // Placeholder for actual implementation of converting a TsonDefinition to bytes
        vec![]
    }
    pub fn from_reader<R: std::io::Read>(reader: &mut R) -> Result<Self, TsonError> {
        // Placeholder for actual implementation of reading a TsonDefinition from a reader
        Ok(Self {
            def_ends_at: 0,
            def_type: TsonData::Null,
            index_name: 0,
            fields: None,
        })
    }
    pub fn to_writer<W: std::io::Write>(&self, writer: &mut W) -> Result<(), TsonError> {
        // Placeholder for actual implementation of writing a TsonDefinition to a writer
        Ok(())
    }
    pub fn validate(&self) -> Result<(), TsonError> {
        // Placeholder for actual implementation of validating a TsonDefinition
        Ok(())
    }
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
    pub fn display(&self) {
        println!("TSON Definition:");
        println!("Definition Ends At: {}", self.def_ends_at);
        println!("Definition Type: {:?}", self.def_type);
        println!("Index Name: {}", self.index_name);
        if let Some(fields) = &self.fields {
            println!("Fields:");
            for (field_name, field_type) in fields {
                println!("  {}: {:?}", field_name, field_type);
            }
        }
    }
}

// A chunk is a block of TsonData that uses a TsonDefinition as its structure.
pub struct TsonChunk {
    pub definition_index: i32, // Index into the definition block that describes the structure of this chunk
    pub data: Vec<TsonData>, // The actual data for this chunk, structured according to the referenced definition
}

#[derive(Debug, PartialEq)]
pub struct TsonDocument {
    pub header: TsonHeader,
    pub definitions: Vec<TsonDefinition>,
    pub data: Vec<TsonData>,
}