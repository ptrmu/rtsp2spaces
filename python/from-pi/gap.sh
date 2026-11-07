#!/bin/bash
current_date=$(date +"%Y%m%d")
python3 ~/Work/grab_and_push/grab_and_push.py >> ~/Work/grab_and_push/logs/gap$current_date.log
#python3 ~/Work/grab_and_push/grab_aruco.py >> ~/Work/grab_and_push/logs_aruco/aru$current_date.log

