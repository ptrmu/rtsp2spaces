import boto3
import cv2
import datetime as dt
import json
import pathlib as pl
import shutil

# Create a VideoCapture object
cap = cv2.VideoCapture("rtsp://admin:FnJmtZsAsZ9.@192.168.1.25/Preview_02_main")

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
local_file = script_dir.joinpath("stake_image.jpg")
save_file = script_dir.joinpath("files", "mazama_stake_" + now_local_filename + ".jpg")
metadata_file = script_dir.joinpath("stake_image.json")

shape = frame.shape
height = shape[0]
width = shape[1]
new_width = height if height < width else width
origin = max(0, int(width / 2.0 - new_width / 2.0))

# Save the image and make a copy with a unique filename
cv2.imwrite(str(local_file), frame[:, origin:origin+new_width])
shutil.copy2(local_file, save_file)

key_id = str("DO00KD6F2YQCMVAWFF8N")
secret_access_key = str("bategMqsRz+Ht4TKxPeVGleJII4C/n3VZLsC0kDxxDQ")
region_name = "sfo3"
bucket_name = "stake-images"
image_upload_name = "mazama/latest/stake_image.jpg"
metadata_upload_name = "mazama/latest/stake_image.json"

endpoint_url = 'https://' + str(region_name) + '.digitaloceanspaces.com'
image_base_url = 'https://' + bucket_name + "." + str(region_name) + '.digitaloceanspaces.com'

# Create the metadata file
metadata = {
    "image": {
        "url": image_base_url + "/" + image_upload_name,
        "alt": "Mazama Snow Stake"
    },
    "timestamp": int(now_utc.timestamp()),
    "valid_until": int((now_utc + dt.timedelta(seconds=300)).timestamp())
}
with open(metadata_file, 'w') as json_file:
    json.dump(metadata, json_file, indent=4)

# Upload the image and metadata files to spaces
session = boto3.session.Session()
client = session.client('s3',
                        region_name=str(region_name),
                        endpoint_url=endpoint_url,
                        aws_access_key_id=key_id,
                        aws_secret_access_key=secret_access_key)


try:
    client.upload_file(local_file, bucket_name, image_upload_name,
                       ExtraArgs={"ContentType": "image/jpeg", "CacheControl": "max-age=60", "ACL": "public-read"})

except Exception as e:
    print(now_local_str, "Error occurred uploading image. Check Space name, etc", e)
    exit(1)

try:
    client.upload_file(metadata_file, bucket_name, metadata_upload_name,
                       ExtraArgs={"ContentType": "application/json", 
                                  "CacheControl": "max-age=60", 
                                  "ACL": "public-read",
                                  "Metadata": {
                                               "Access-Control-Allow-Origin": "*"}})

except Exception as e:
    print(now_local_str, "Error occurred uploading metadata. Check Space name, etc", e)
    exit(1)

print(now_local_str, "Success")



