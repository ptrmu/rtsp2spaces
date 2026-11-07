#!/bin/bash
current_date=$(date +"%Y%m%d")
python3 ~/Work/grab_and_push/grab_and_push.py >> ~/Work/grab_and_push/gap$current_date.log
