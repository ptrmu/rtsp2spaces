import boto3
import cv2
import datetime as dt
import json
import pathlib as pl
import shutil

# Create a VideoCapture object
cap = cv2.VideoCapture("rtsp://admin:.5Yd!o$N&@mxN&LWRiwW@192.168.1.183/Preview_02_main")

# Check if the camera opened successfully
if not cap.isOpened():
    print("Cannot open camera")
    exit()

ret0 = cap.grab()
ret1 = cap.grab()
ret2 = cap.grab()

now_utc = dt.datetime.now(dt.timezone.utc)
now_local = dt.datetime.now()

ret3, frame = cap.retrieve()

# Release the capture
cap.release()

now_local_str = now_local.strftime("%m/%d/%Y-%H:%M:%S")
now_local_filename = now_local.strftime("%Y%m%d%H%M")

if not (ret0 and ret1 and ret2 and ret3):
    print(now_local_str, "Can't receive frame (stream end?). Exiting ...")
    exit(1)

# Find where this script is running from
script_dir = pl.Path(__file__).resolve().parent
local_file = script_dir.joinpath("aruco_stake_image.jpg")
save_file = script_dir.joinpath("files_aruco", "aruco_stake_" + now_local_filename + ".jpg")

shape = frame.shape
height = shape[0]
width = shape[1]
new_width = height if height < width else width
origin = max(0, int(width / 2.0 - new_width / 2.0))

# Save the image and make a copy with a unique filename
cv2.imwrite(str(local_file), frame[:, origin:origin+new_width])
shutil.copy2(local_file, save_file)

print(now_local_str, "Success")



