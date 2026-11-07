#!/bin/bash

# Exit immediately if any command returns a non-zero status
set -e

# Check if the user provided a directory as an argument
if [ $# -lt 1 ]; then
    echo "Usage: $0 <directory>"
    echo "The directory argument determines:"
    echo "  'files'       - Search for 'mazama_stake_2025mmdd*.jpg'"
    echo "  'files_aruco' - Search for 'aruco_stake_2025mmdd*.jpg'"
    exit 1
fi

# Get the directory from the argument
search_dir="$1"

# Verify if the provided argument is a valid directory
if [ ! -d "$search_dir" ]; then
    echo "Error: $search_dir is not a valid directory."
    exit 1
fi

# Determine file search pattern and prefix length based on the directory name
case "$(basename "$search_dir")" in
    "files")
        file_pattern="mazama_stake_2025[0-1][0-9][0-3][0-9]*.jpg"
        prefix_length=13  # Length of "mazama_stake_"
        ;;
    "files_aruco")
        file_pattern="aruco_stake_2025[0-1][0-9][0-3][0-9]*.jpg"
        prefix_length=12  # Length of "aruco_stake_"
        ;;
    *)
        echo "Error: Invalid directory name. The directory should be named either 'files' or 'files_aruco'."
        exit 1
        ;;
esac

# Iterate through all files in the given directory matching the pattern
for file in "$search_dir"/$file_pattern; do
    # Check if there are any matching files
    if [ -e "$file" ]; then
        # Extract the '2025mmdd' portion of the filename
        base=$(basename "$file")
        folder_name="${base:${prefix_length}:8}" # Extract 8 characters starting after the prefix

        # Define the target directory based on the extracted date portion
        target_dir="$search_dir/$folder_name"

        # Create the directory if it doesn't exist
        if [ ! -d "$target_dir" ]; then
            mkdir "$target_dir"
        fi

        # Move the file to the target directory
        mv "$file" "$target_dir"
    fi
done

echo "All matching files have been moved to their respective directories."
