#!/bin/bash

# Chart Component Test Script
# This script specifically tests the chart component to catch compilation issues early

echo "🧪 Running Chart Component Tests..."
echo "=================================="

# Test the frontend package specifically
cd frontend

echo "📦 Testing frontend package..."
if cargo test --lib components::transactions::rust_chart; then
    echo "✅ Chart component tests PASSED"
    echo
    echo "📊 Chart-specific test results:"
    echo "- ✅ Component compilation"
    echo "- ✅ Props creation"
    echo "- ✅ Data preparation logic"
    echo "- ✅ Empty data handling"
    echo "- ✅ Invalid date handling"
    echo "- ✅ Plotters color/lifetime handling"
    echo
    echo "🎉 All chart tests passed! Chart component is ready for development."
else
    echo "❌ Chart component tests FAILED"
    echo
    echo "💡 This script helps catch:"
    echo "- Compilation errors (like the plotters lifetime issues we had)"
    echo "- Data structure mismatches"
    echo "- Import/dependency issues"
    echo "- Basic component functionality"
    echo
    echo "🔧 Run this script before pushing changes to catch issues early!"
    exit 1
fi

echo
echo "🚀 To run all tests: cargo test"
echo "🎯 To run only chart tests: cargo test components::transactions::rust_chart"
echo "⚡ To run this script: ./test-chart.sh" 