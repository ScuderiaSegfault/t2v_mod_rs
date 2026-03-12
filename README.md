# T2V Module Userspace Driver and Debugging Tools

![GitHub License](https://img.shields.io/github/license/ScuderiaSegfault/t2v_module_rs)

This repository contains the userspace driver for the RoboRacer T2V module and tools to test the module.
It consists of the following modules:

* `t2v_driver` - `systemd` capable userspace driver for `t2v_mod`
* `t2v_driver_cli` - CLI interface to the userspace driver
* `t2v_driver_proto` - Protocol definitions for the communication with `t2v_driver`
* `t2v_module` - Low-level interface to the `t2v_mod`

## Building

To build the components in this repository, you need a recent version of `rustc` and `cargo`.
Minimum supported Rust version (MSRV) is `1.88.0`.
You can find installation instructions for Rust and the toolchain at [rustup](https://rustup.rs/).

To build the driver, run:

```bash
cargo build --package t2v_driver --release --features systemd
```

The compiled driver will be located at `target/release/t2v_driver`. 
If you wish to build without `systemd` support, remove `--features systemd` from the command line.

To build the driver CLI, run:

```bash
cargo build --package t2v_driver_cli --release
```

The tool is then located at `target/release/t2v_driver_cli`.

## Installation

Depending on personal preference, you can copy the resulting binaries to a globally accessible location or add it to your path.

### `udev` Rules

Copy the rule file `udev/t2v_module.rules` to `/etc/udev/rules.d` to set the permissions of the T2V module correctly.
Replace the values for `group` and `user` to fit your requirements. (Typically this will be your username for both)
This will configure the device to have correct access permissions for the group and user you have provided.

Afterward, either reboot your device or reload the `udev` daemon.

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

You will most likely have to re-plug your T2V module for the permission to work.

### `systemd`

If you want to use the userspace driver with `systemd` you can use the files provided in the `systemd/` directory.
Copy the files to `/etc/systemd/system` and modify the file `t2v_module.service` to point to the directory that contains the driver binary.
Reload the daemon to make it aware of the new unit definitions.

```bash
sudo systemctl daemon-reload
```

To start the driver, run:

```bash
sudo systemctl start t2v_module.service
```

This will automatically start (create) the service socket and provide it to the unit.
If you want to change the socket path, modify the path in `t2v_module.socket` and reload the daemon.

To start the driver on every start, run:

```bash
sudo systemctl enable t2v_module.service
```

## Usage

The driver uses default values wherever possible, but some values still need to be provided.
When looking at the help message, we can see which arguments we can provide.

```
Usage: t2v_driver [OPTIONS]

Options:
      --systemd                    [env: SYSTEMD=]
  -s, --socket-file <SOCKET_FILE>  [env: SOCKET_FILE=]
      --vendor-id <VENDOR_ID>      [env: VENDOR_ID=] [default: 21589]
      --product-id <PRODUCT_ID>    [env: PRODUCT_ID=] [default: 6417]
  -h, --help                       Print help
```

The vendor and product id are the default values used in the [T2V firmware](https://github.com/ScuderiaSegfault/t2v_mod_fw).
In `systemd` mode, the driver either expects a socket file from `systemd` or one socket file from the command line.
For testing, we can provide a socket file, using the `-s/--socket-file` option like this:

```bash
target/release/t2v_driver -s t2v_socket.sock
```

Depending on the configuration, the driver uses either `journald` for logging or a formatted logger to stdout.
The formatted logger severity can be controlled using the `RUST_LOG` environment variable.
To run with `INFO` severity, run:

```bash
RUST_LOG=info target/release/t2v_driver -s t2v_socket.sock
```

If you built the driver with `systemd` support, it will print the message `systemd support requested but not detected, reverting to normal operation`.
This message indicates, that it is running without `systemd` support.

As soon as the driver is running, we can use the CLI tool to inspect the driver.
The help message indicates the arguments we can provide:

```
Usage: t2v_driver_cli [OPTIONS] <DRIVER_SOCKET>

Arguments:
  <DRIVER_SOCKET>  

Options:
  -h, --help             Print help
```

The tool automatically generates a temporary socket file, so make sure that it has access to the temporary file directory. 
To test the tool, we can run:

```bash
target/release/t2v_driver_cli t2v_socket.sock
```

Similarly to the driver, you can set the logging severity by setting the `RUST_LOG` environment variable.