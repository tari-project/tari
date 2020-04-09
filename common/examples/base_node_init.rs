use structopt::StructOpt;

#[derive(StructOpt, Debug)]
/// The reference Tari cryptocurrency base node implementation
struct Arguments {
    /// Custom application parameters might eb specified as usual
    #[structopt(long, default_value = "any structopt options allowed")]
    my_param: String,
    #[structopt(flatten)]
    bootstrap: tari_common::ConfigBootstrap,
}

fn main() {
    Arguments::clap().print_help().expect("failed to print help");
    let mut args = Arguments::from_args();
    args.bootstrap.init_dirs().expect("failed to initialize configs");
    println!("");
    dbg!(args);
}
