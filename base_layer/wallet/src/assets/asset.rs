
#[derive(Clone)]
pub struct Asset {
    name : String,
    registration_output_status: String,
}

impl Asset {
    pub fn new(name: String, registration_output_status: String)  -> Self{
       Self {
           name,
           registration_output_status
       }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn registration_output_status(&self) -> &str {
        self.registration_output_status.as_str()
    }
}
