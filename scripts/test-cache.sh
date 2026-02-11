#!/bin/bash
# Test script for cache invalidation system
#
# Usage:
#   ./scripts/test-cache.sh [--verbose] [--scenario <name>]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Parse arguments
VERBOSE=false
SCENARIO=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --scenario|-s)
            SCENARIO="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -v, --verbose     Show detailed test output"
            echo "  -s, --scenario    Run specific scenario (a, b, c, or all)"
            echo "  -h, --help        Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                Run all cache tests"
            echo "  $0 --verbose      Run with detailed output"
            echo "  $0 -s a           Run only Scenario A tests"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

echo -e "${BLUE}=== Cache Invalidation Test Suite ===${NC}"
echo ""

# Function to run tests
run_test() {
    local test_name=$1
    local description=$2

    echo -e "${YELLOW}Testing: ${description}${NC}"

    if [ "$VERBOSE" = true ]; then
        cargo test --test "$test_name" -- --nocapture
    else
        cargo test --test "$test_name" 2>&1 | grep -E "(test result|passed|failed)"
    fi

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ ${description} passed${NC}"
    else
        echo -e "${RED}✗ ${description} failed${NC}"
        return 1
    fi
    echo ""
}

# Run specific scenario or all tests
if [ -n "$SCENARIO" ]; then
    case $SCENARIO in
        a|A)
            echo "Running Scenario A: Method-Only Change"
            echo ""
            if [ "$VERBOSE" = true ]; then
                cargo test test_scenario_a_method_only_change -- --nocapture
            else
                cargo test test_scenario_a_method_only_change
            fi
            ;;
        b|B)
            echo "Running Scenario B: Children-Only Change"
            echo ""
            if [ "$VERBOSE" = true ]; then
                cargo test test_scenario_b_children_only_change -- --nocapture
            else
                cargo test test_scenario_b_children_only_change
            fi
            ;;
        c|C)
            echo "Running Scenario C: Both Change"
            echo ""
            if [ "$VERBOSE" = true ]; then
                cargo test test_scenario_c_both_change -- --nocapture
            else
                cargo test test_scenario_c_both_change
            fi
            ;;
        all|ALL)
            run_test "cache_invalidation_test" "Core Cache Invalidation"
            run_test "configurable_backend_test" "Configurable Backend"
            ;;
        *)
            echo -e "${RED}Unknown scenario: $SCENARIO${NC}"
            echo "Valid scenarios: a, b, c, all"
            exit 1
            ;;
    esac
else
    # Run all tests
    echo -e "${BLUE}Running all cache tests...${NC}"
    echo ""

    run_test "cache_invalidation_test" "Core Cache Invalidation"
    run_test "configurable_backend_test" "Configurable Backend"

    echo -e "${GREEN}=== All Cache Tests Passed ===${NC}"
fi

# Run example programs if verbose
if [ "$VERBOSE" = true ] && [ -z "$SCENARIO" ]; then
    echo ""
    echo -e "${BLUE}=== Example Programs ===${NC}"
    echo ""

    echo -e "${YELLOW}Example 1: Generate from config${NC}"
    cargo run --example generate_from_config tests/test_scenarios/scenario_a_initial.json
    echo ""

    echo -e "${YELLOW}Example 2: Compare configs${NC}"
    cargo run --example compare_configs \
        tests/test_scenarios/scenario_a_initial.json \
        tests/test_scenarios/scenario_a_modified.json
    echo ""
fi

echo -e "${GREEN}✓ Cache test suite complete${NC}"
