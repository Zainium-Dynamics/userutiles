trigger: Universal Application & Script Runner

`trigger` is a core component of the Zutils toolchain. It acts as a universal, highly-intelligent execution dispatcher for Zainium OS. It seamlessly distinguishes between system binaries and source code scripts, automatically routing them to their correct interpreters without requiring explicit declarations.

FEATURES
--------
* Universal Execution: Run any application OR any programming script (Python, Rust, Bash, JS, C++, etc.) with a single unified command.
* Dynamic Discovery: Scans the live system environment at runtime. No hardcoded paths.
* Safe & Secure: Built-in root-user detection, EACCES permission distinction, and zero panic points.
* Cyber-Tech Output: Clean, structured diagnostic UI for execution tracking and PID monitoring.


USAGE
-----
 trigger [OPTIONS] [TARGET]...

OPTIONS:
 --dry-run Show detected type and resolved path without executing.
 -v, --verbose Show info-level logs and detailed success diagnostics.
 -h, --help Print help information.


EXAMPLES
--------
Launch a system application (e.g., VS Code):
 $ trigger code

Execute a script file (e.g., Python, Bash, Node.js):
 $ trigger main.py
 $ trigger build.sh
 $ trigger server.js

Pass arguments through to the target:
 $ trigger code /path/to/project
 $ trigger deploy.sh --env production

List available applications or handlers (If supported by your config):
 $ trigger list apps
 $ trigger list handlers


TERMINAL OUTPUT AESTHETIC
-------------------------
`trigger` provides deterministic, structured feedback before execution.

Example of launching an application:

 Resolving...
 Target : code
 Type : Application
 Path : /usr/bin/code

 Executing...
 Command: code
 User : ali-zain
 Mode : normal

 [OK] Process started successfully
 PID : 1141828


ERROR HANDLING
--------------
`trigger` exits cleanly with specific codes rather than panicking:
* Exit 0: Success
* Exit 2: Application not found in PATH
* Exit 3: Script file does not exist
* Exit 4: Permission Denied (EACCES)
* Exit 5: Binary found but failed to launch
* Exit 7: Root execution forbidden (GUI app protection)


ARCHITECTURE
------------
By passing all arguments directly (`trigger <target>`), the parser instantly evaluates if the target exists as a local file (routing to the script handler engine) or if it exists in the system `$PATH` (routing to the binary execution engine).
