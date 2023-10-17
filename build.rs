use hex_literal::hex;
use md5::{Digest, Md5};
use std::{env, path::PathBuf};

const KERNEL_VER: &str = "3.8.0";
const KERNEL_MD5: [u8; 16] = hex!("4f307cae0fcda9ada7b0e3984713fd94");

fn main() {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-changed=src/pros.h");

	let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	let kernel_path = out_path.join("kernel");

	// Download the kernel and extract it
	download_and_extract();

	// Add link search paths for the firmware and link
	println!(
		"cargo:rustc-link-search={}",
		kernel_path.join("firmware").display()
	);

	// Gather C headers
	let inlude_paths = gather_c_headers();

	// Generate bindings
	let mut bindings = bindgen::Builder::default()
		.header("src/pros.h")
		.clang_args(&["-target", "arm-none-eabi"])
		.clang_args(inlude_paths)
		.ctypes_prefix("core::ffi")
		.layout_tests(false)
		.generate_comments(false)
		.use_core();

	// blacklist stuff as needed
	bindings = bindings.blocklist_item("vision_object_s_t");

	bindings
		.generate()
		.expect("unable to generate bindings")
		.write_to_file(out_path.join("bindings.rs"))
		.expect("failed to write bindings");
}

fn download_and_extract() {
	let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	let zip_path = out_path.join(format!("kernel@{}.zip", KERNEL_VER));
	let kernel_path = out_path.join("kernel");

	// check zip MD5 matches
	let matches = match std::fs::read(&zip_path) {
		Ok(bytes) => {
			let mut hasher = Md5::new();
			hasher.update(bytes);
			let md5 = hasher.finalize();

			md5 == KERNEL_MD5.into()
		}
		Err(_) => false,
	};

	if !matches {
		// println!("cargo:warning=Missing PROS kernel, had to be redownload.");

		// download release file
		let bytes = match reqwest::blocking::get(format!(
			"https://github.com/purduesigbots/pros/releases/download/{}/kernel@{}.zip",
			KERNEL_VER, KERNEL_VER
		)) {
			Ok(resp) if resp.status().is_success() => resp.bytes().unwrap(),
			_ => {
				eprintln!("error: failed to download kernel zip");
				std::process::exit(1);
			}
		};

		// save to file
		std::fs::write(&zip_path, bytes).unwrap();
	}

	// extract kernel zip
	let zip_file = std::fs::File::open(&zip_path).expect("failed to open kernel zip file");

	zip::read::ZipArchive::new(&zip_file)
		.unwrap()
		.extract(&kernel_path)
		.expect("failed to extract zip");
}

fn gather_c_headers() -> Vec<String> {
	let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	let kernel_path = out_path.join("kernel");

	// Get the C std headers to make sure types are generated correctly
	let command = match std::process::Command::new("arm-none-eabi-gcc")
		.args(["-E", "-Wp,-v", "-xc", "/dev/null"])
		.output()
	{
		Ok(c) => c,
		Err(_) => {
			eprintln!("error: arm-none-eabi-gcc not found in path, is it installed?");
			std::process::exit(1);
		}
	};

	let mut include_paths = Vec::new();
	let mut in_lines = false;

	// extract lines from stderr
	let stderr = std::str::from_utf8(&command.stderr).unwrap();
	for err in stderr.lines() {
		if err == "End of search list." {
			in_lines = false;
		}
		if in_lines {
			include_paths.push(format!("-I{}", err.trim()))
		}
		if err == "#include <...> search starts here:" {
			in_lines = true;
		}
	}

	// add pros include dir
	include_paths.push(format!(
		"-I{}",
		kernel_path.join("include").to_string_lossy()
	));

	include_paths
}
