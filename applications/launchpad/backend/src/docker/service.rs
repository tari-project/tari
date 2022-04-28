
lazy_static!{
    pub static ref docker: Docker = Docker::connect_with_local_defaults();
}