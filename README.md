# A Tui/Web-based ACPI Table Editor

---

![acpied](/images/acpied.png)

## Background

[Upgrading ACPI tables via initrd](https://www.kernel.org/doc/html/latest/admin-guide/acpi/initrd_table_override.html)

## Build and Install

```shell
# build
$ make

# install
$ make install
```

## Usage

```shell
acpied
```

### Key Bindings

#### Navigate dsl files

- `Up`: previous dsl file.
- `Down`: next dsl file.

### Edit dsl file

Support for vim-like key bindings.

### Apply modifications.

Applying means assembling the tweaked dsl file, adding it to initrd file,
creating a new initrd file, and setting it as the initrd file used by the
boot entry.

To apply the modifications, press `Ctrl`+`A`.

## Log file

Log file path: _/var/log/acpied.log_

## Todo:

- Check following kernel configurations.
  - CONFIG_ARCH_HAS_ACPI_TABLE_UPGRADE=y
  - CONFIG_ACPI_TABLE_UPGRADE=y
- Implement web mode.
