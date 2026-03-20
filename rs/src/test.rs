use std::fs::File;
use std::path::PathBuf;

pub struct Test {
    name: String,
}

impl Test {
    pub fn load_test_file(path: &PathBuf) {
        let file = File::open(path)?;

        // 2. Wrap it in a BufReader for efficiency
        let reader = BufReader::new(file);

        // 3. Deserialize directly from the reader
        let config: Config = serde_yaml::from_reader(reader)?;
    }
}
