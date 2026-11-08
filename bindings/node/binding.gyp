{
  "targets": [
    {
      "target_name": "geoda_native",
      "type": "loadable_module",
      "sources": ["src/addon.c"],
      "include_dirs": ["src", "../../crates/capi/include"],
      "variables": {
        "lib_dir": "<!(node -p \"process.env.GEODA_LIB_DIR || require('path').resolve('../../target/debug')\")"
      },
      "cflags": ["-Wall", "-Wextra", "-Werror", "-std=c11"],
      "libraries": ["-L<(lib_dir)", "-lgeoda", "-Wl,-rpath,<(lib_dir)"]
    }
  ]
}
