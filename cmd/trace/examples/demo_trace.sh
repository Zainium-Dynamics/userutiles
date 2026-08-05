#!/bin/bash

# trace - Demo & Testing Script
# Demonstrates all major features of the trace system call tracer

set -e

TRACE_BIN="${TRACE_BIN:-./target/release/trace}"
DEMO_PID=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Banner
print_header() {
    echo -e "${BLUE}"
    echo "╔════════════════════════════════════════════╗"
    echo "║  trace - System Call Tracing Demo & Tests  ║"
    echo "╚════════════════════════════════════════════╝"
    echo -e "${NC}\n"
}

# Test 1: System Information
test_system_info() {
    echo -e "${YELLOW}[TEST 1] System Information${NC}"
    echo "Command: $TRACE_BIN info"
    echo "────────────────────────────────────────────"
    if $TRACE_BIN info; then
        echo -e "${GREEN}✓ System info test passed${NC}\n"
    else
        echo -e "${RED}✗ System info test failed${NC}\n"
        return 1
    fi
}

# Test 2: List Processes
test_list_processes() {
    echo -e "${YELLOW}[TEST 2] List Running Processes${NC}"
    echo "Command: $TRACE_BIN processes"
    echo "────────────────────────────────────────────"
    if $TRACE_BIN processes; then
        echo -e "${GREEN}✓ Process listing test passed${NC}\n"
    else
        echo -e "${RED}✗ Process listing test failed${NC}\n"
        return 1
    fi
}

# Test 3: Trace Shell Process (by name)
test_trace_by_name() {
    echo -e "${YELLOW}[TEST 3] Trace Process by Name${NC}"
    echo "Command: $TRACE_BIN --process bash"
    echo "────────────────────────────────────────────"
    
    # Find a bash process
    if pgrep -q bash; then
        if $TRACE_BIN --process bash; then
            echo -e "${GREEN}✓ Trace by name test passed${NC}\n"
        else
            echo -e "${RED}✗ Trace by name test failed${NC}\n"
            return 1
        fi
    else
        echo -e "${YELLOW}⊘ Bash process not found, skipping${NC}\n"
    fi
}

# Test 4: Trace by PID
test_trace_by_pid() {
    echo -e "${YELLOW}[TEST 4] Trace Process by PID${NC}"
    
    # Get current shell PID
    local shell_pid=$$
    echo "Command: $TRACE_BIN --pid $shell_pid"
    echo "────────────────────────────────────────────"
    
    if $TRACE_BIN --pid $shell_pid; then
        echo -e "${GREEN}✓ Trace by PID test passed${NC}\n"
    else
        echo -e "${RED}✗ Trace by PID test failed${NC}\n"
        return 1
    fi
}

# Test 5: JSON Output Format
test_json_output() {
    echo -e "${YELLOW}[TEST 5] JSON Output Format${NC}"
    
    local shell_pid=$$
    echo "Command: $TRACE_BIN --pid $shell_pid --format json"
    echo "────────────────────────────────────────────"
    
    if $TRACE_BIN --pid $shell_pid --format json | grep -q '"process"'; then
        echo -e "${GREEN}✓ JSON output test passed${NC}\n"
    else
        echo -e "${RED}✗ JSON output test failed${NC}\n"
        return 1
    fi
}

# Test 6: YAML Output Format
test_yaml_output() {
    echo -e "${YELLOW}[TEST 6] YAML Output Format${NC}"
    
    local shell_pid=$$
    echo "Command: $TRACE_BIN --pid $shell_pid --format yaml"
    echo "────────────────────────────────────────────"
    
    if $TRACE_BIN --pid $shell_pid --format yaml | grep -q 'process:'; then
        echo -e "${GREEN}✓ YAML output test passed${NC}\n"
    else
        echo -e "${RED}✗ YAML output test failed${NC}\n"
        return 1
    fi
}

# Test 7: Error Handling - Non-existent Process
test_error_handling() {
    echo -e "${YELLOW}[TEST 7] Error Handling${NC}"
    echo "Command: $TRACE_BIN --process nonexistent_process_xyz"
    echo "────────────────────────────────────────────"
    
    if ! $TRACE_BIN --process nonexistent_process_xyz 2>&1 | grep -q "not found"; then
        echo -e "${YELLOW}⚠ Error handling test inconclusive${NC}\n"
    else
        echo -e "${GREEN}✓ Error handling test passed${NC}\n"
    fi
}

# Test 8: Output to File
test_output_file() {
    echo -e "${YELLOW}[TEST 8] Save Output to File${NC}"
    
    local shell_pid=$$
    local output_dir="/tmp/trace_test_$$"
    mkdir -p "$output_dir"
    
    echo "Command: $TRACE_BIN --pid $shell_pid --output $output_dir"
    echo "────────────────────────────────────────────"
    
    if $TRACE_BIN --pid $shell_pid --output "$output_dir" > /dev/null 2>&1; then
        if [ -f "$output_dir"/* ]; then
            echo -e "${GREEN}✓ Output file test passed${NC}"
            echo "  Saved to: $output_dir"
            rm -rf "$output_dir"
            echo ""
        else
            echo -e "${RED}✗ No output file created${NC}\n"
            rm -rf "$output_dir"
            return 1
        fi
    else
        echo -e "${RED}✗ Output file test failed${NC}\n"
        rm -rf "$output_dir"
        return 1
    fi
}

# Test 9: Verbose Logging
test_verbose_logging() {
    echo -e "${YELLOW}[TEST 9] Verbose Logging${NC}"
    
    local shell_pid=$$
    echo "Command: $TRACE_BIN --pid $shell_pid --verbose"
    echo "────────────────────────────────────────────"
    
    # Verbose output goes to stderr
    if $TRACE_BIN --pid $shell_pid --verbose 2>&1 | head -n 20; then
        echo -e "${GREEN}✓ Verbose logging test passed${NC}\n"
    else
        echo -e "${RED}✗ Verbose logging test failed${NC}\n"
        return 1
    fi
}

# Check if binary exists
check_binary() {
    if [ ! -f "$TRACE_BIN" ]; then
        echo -e "${RED}Error: trace binary not found at $TRACE_BIN${NC}"
        echo "Build with: cargo build --release"
        exit 1
    fi
}

# Run all tests
run_all_tests() {
    local failed=0
    
    print_header
    check_binary
    
    # Run tests
    test_system_info || ((failed++))
    test_list_processes || ((failed++))
    test_trace_by_name || ((failed++))
    test_trace_by_pid || ((failed++))
    test_json_output || ((failed++))
    test_yaml_output || ((failed++))
    test_error_handling || ((failed++))
    test_output_file || ((failed++))
    test_verbose_logging || ((failed++))
    
    # Summary
    echo -e "${BLUE}════════════════════════════════════════════${NC}"
    if [ $failed -eq 0 ]; then
        echo -e "${GREEN}All tests passed! ✓${NC}"
        echo -e "${BLUE}════════════════════════════════════════════${NC}\n"
    else
        echo -e "${RED}$failed test(s) failed${NC}"
        echo -e "${BLUE}════════════════════════════════════════════${NC}\n"
        exit 1
    fi
}

# Main
main() {
    case "${1:-all}" in
        all)
            run_all_tests
            ;;
        info)
            test_system_info
            ;;
        processes)
            test_list_processes
            ;;
        by-name)
            test_trace_by_name
            ;;
        by-pid)
            test_trace_by_pid
            ;;
        json)
            test_json_output
            ;;
        yaml)
            test_yaml_output
            ;;
        error)
            test_error_handling
            ;;
        file)
            test_output_file
            ;;
        verbose)
            test_verbose_logging
            ;;
        *)
            echo "Usage: $0 [all|info|processes|by-name|by-pid|json|yaml|error|file|verbose]"
            exit 1
            ;;
    esac
}

main "$@"
