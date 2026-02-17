#!/bin/bash
# Test script for OSC 9;4 progress bar

echo "Testing Rio progress bar (OSC 9;4)"
echo "================================="
echo ""

# Function to show progress bar
show_progress() {
    local state=$1
    local progress=$2
    printf '\e]9;4;%d;%d\e\\' "$state" "$progress"
}

# Function to hide progress bar
hide_progress() {
    printf '\e]9;4;0\e\\'
}

echo "1. Normal progress bar (blue) - 0% to 100%"
for i in $(seq 0 5 100); do
    show_progress 1 "$i"
    sleep 0.05
done
sleep 1

echo "2. Error state (red) at 75%"
show_progress 2 75
sleep 2

echo "3. Warning state (yellow) at 50%"
show_progress 4 50
sleep 2

echo "4. Indeterminate state (pulsing)"
show_progress 3 0
sleep 3

echo "5. Back to normal progress"
for i in $(seq 0 10 100); do
    show_progress 1 "$i"
    sleep 0.1
done
sleep 1

echo "6. Hiding progress bar"
hide_progress
sleep 1

echo ""
echo "Test complete!"
