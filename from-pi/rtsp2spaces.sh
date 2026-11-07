#!/bin/bash
current_date=$(date +"%Y%m%d")
python3 ~/Work/grab_and_push/rtsp2spaces.py aruco_cam_secrets --verbose
# &>> ~/Work/grab_and_push/gap$current_date.log

