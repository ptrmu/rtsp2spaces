import argparse as ap
import boto3 as b3
import botocore.config as bc
import botocore.exceptions as be
import cv2
import datetime as dt
import json
import logging
import pathlib as pl


class StdMsg:
    def __init__(self, verbose=False):
        # Initialize necessary attributes
        now_exec_local = dt.datetime.now()
        self.app_name = pl.Path(__file__).stem + now_exec_local.strftime('-%H:%M:%S')

        # Set up the logger with the appropriate level
        self.logger = logging.getLogger(self.app_name)
        self.verbose = verbose  # Flag to control verbosity
        self._set_logging_level()  # Configure level based on verbose flag

        # Standardize the logger format for output
        handler = logging.StreamHandler()
        formatter = logging.Formatter(fmt="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
                                      datefmt="%m/%d/%Y-%H:%M:%S")
        handler.setFormatter(formatter)
        self.logger.addHandler(handler)
        self.logger.propagate = False  # Prevent duplicate logs

    def _set_logging_level(self):
        """Set the logging level based on the verbose flag."""
        self.logger.setLevel(logging.DEBUG if self.verbose else logging.WARNING)

    def set_verbose(self, verbose: bool):
        """Update the verbose flag dynamically."""
        self.verbose = verbose
        self._set_logging_level()
        self.info("Logging level updated to DEBUG" if self.verbose else "Logging level updated to WARNING")

    # Mimic logger methods for different levels
    def debug(self, msg: str):
        self.logger.debug(msg)

    def info(self, msg: str):
        self.logger.info(msg)

    def warning(self, msg: str):
        self.logger.warning(msg)

    def error(self, msg: str):
        self.logger.error(msg)
        raise Exception("Error")

    def critical(self, msg: str):
        self.logger.critical(msg)
        raise Exception("Critical")

    def __call__(self, msg: str):
        print(f"{self.app_name} - {msg}")


class CustomArgumentParser(ap.ArgumentParser):
    def __init__(self, *args, std_msg, **kwargs):
        super().__init__(*args, **kwargs)
        self.std_msg = std_msg  # Store the extra information

    def error(self, message):
        self.std_msg.error(message)


def parse_args(std_msg: StdMsg) -> ap.Namespace:
    parser = CustomArgumentParser(std_msg=std_msg)

    parser.add_argument("--verbose",
                        type=bool,
                        help="turn on verbose mode",
                        default=False)

    parser.add_argument("secrets_filename",
                        type=str,
                        help="JSON file with secrets: rtsp_user, rtsp_pw, spaces_key, spaces_access_key")

    parser.add_argument("--host",
                        type=str,
                        help="rtsp host name",
                        default="192.168.1.25")
    parser.add_argument("--stream_selector",
                        type=str,
                        help="rtsp stream name/selector",
                        default="Preview_02_main")

    parser.add_argument("--image_name_prefix",
                        help="local image filename prefix",
                        default='files/mazama_stake_')
    parser.add_argument("--metadata_name",
                        help="local metadata filename",
                        default='mazama_stake')

    parser.add_argument("--image_expiration_seconds",
                        help="Length of time in seconds before a new image is uploaded",
                        type=int,
                        default=60)
    parser.add_argument("--metadata_image_alt",
                        help="alt text for img tag in browser",
                        type=str,
                        default="Mazama Snow Stake")

    parser.add_argument("--region",
                        type=str,
                        help="spaces region name",
                        default="sfo3")
    parser.add_argument("--bucket",
                        type=str,
                        help="spaces bucket name",
                        default="stake-images")
    parser.add_argument("--upload_image_name",
                        type=str,
                        help="spaces name for uploaded image file",
                        default="mazama/latest/stake_image.jpg")
    parser.add_argument("--upload_metadata_name",
                        type=str,
                        help="spaces name for uploaded metadata file",
                        default="mazama/latest/stake_image.json")

    return parser.parse_args()


class Secrets:
    def __init__(self, std_msg: StdMsg, args: ap.Namespace):
        if not pl.Path(args.secrets_filename).exists():
            std_msg.error(f"Secrets file not found at: {args.secrets_filename}")

        with open(args.secrets_filename, 'r') as file:
            secrets = json.load(file)

        # Validate that all required keys exist
        for key in ["rtsp_user", "rtsp_pw", "spaces_key", "spaces_access_key"]:
            if key not in secrets:
                std_msg.error(f"{key} is missing from the secrets file.")

        self.rtsp_user = secrets["rtsp_user"]
        self.rtsp_pw = secrets["rtsp_pw"]
        self.spaces_key = secrets["spaces_key"]
        self.spaces_access_key = secrets["spaces_access_key"]


class Capture:
    def __init__(self, std_msg: StdMsg, args: ap.Namespace, secrets: Secrets):
        # Create a VideoCapture object
        rtsp_url = f"rtsp://{secrets.rtsp_user}:{secrets.rtsp_pw}@{args.host}/{args.stream_selector}"
        cap = cv2.VideoCapture(rtsp_url)

        # Check if the camera opened successfully
        if not cap.isOpened():
            std_msg.error(f"Can not open stream:{rtsp_url}")

        ret0 = cap.grab()
        ret1 = cap.grab()
        ret2 = cap.grab()

        self.time_utc = dt.datetime.now(dt.timezone.utc)
        self.time_local = dt.datetime.now()

        ret3, self.frame = cap.retrieve()

        # Release the capture
        cap.release()

        if not (ret0 and ret1 and ret2 and ret3):
            std_msg.error("Can't receive frame")


class Filenames:
    def __init__(self, std_msg: StdMsg, args: ap.Namespace, capture):
        # Generate local filenames
        script_dir = pl.Path(__file__).resolve().parent
        capture_local_filename = capture.time_local.strftime("%Y%m%d%H%M")
        self.local_image = script_dir.joinpath(args.image_name_prefix + capture_local_filename).with_suffix(".jpg")
        self.local_metadata = script_dir.joinpath(args.metadata_name).with_suffix(".json")

        # Generate Spaces URLs
        spaces_base_url = f"https://{args.bucket}.{args.region}.digitaloceanspaces.com"
        self.spaces_image_url = f"{spaces_base_url}/{args.upload_image_name}"
        self.spaces_metadata_url = f"{spaces_base_url}/{args.upload_metadata_name}"
        self.spaces_endpoint_url = f"https://{args.region}.digitaloceanspaces.com"

        std_msg.info(f"Local image path: {self.local_image}")
        std_msg.info(f"Local metadata path: {self.local_metadata}")
        std_msg.info(f"Spaces image URL: {self.spaces_image_url}")
        std_msg.info(f"Spaces metadata URL: {self.spaces_metadata_url}")


def save_image(std_msg: StdMsg, filenames: Filenames, capture):
    # Ensure all required inputs exist
    if not all([filenames.local_image, capture.frame.shape]):
        std_msg.error("Missing required parameters for save_image().")

    height, width = capture.frame.shape[:2]
    new_width = min(height, width)
    origin = max(0, int(width / 2.0 - new_width / 2.0))

    try:
        success = cv2.imwrite(str(filenames.local_image), capture.frame[:, origin:origin + new_width])
        if not success:
            std_msg.error(f"Failed to write image to {filenames.local_image}")
    except Exception as e:
        std_msg.error(f"Exception while saving image: {e}")

    std_msg.info(f"Image saved successfully to {filenames.local_image}")


def save_metadata(std_msg: StdMsg, args: ap.Namespace, filenames: Filenames, capture):
    # Ensure all required inputs exist
    if not all([filenames.spaces_image_url, filenames.local_metadata,
                capture.time_utc, args.image_expiration_seconds,
                args.metadata_image_alt]):
        std_msg.error("Missing required parameters for save_metadata().")

    metadata = {
        "image": {
            "url": filenames.spaces_image_url,
            "alt": args.metadata_image_alt
        },
        "timestamp": int(capture.time_utc.timestamp()),
        "valid_until": int((capture.time_utc + dt.timedelta(seconds=args.image_expiration_seconds)).timestamp())
    }

    try:
        with open(filenames.local_metadata, 'w', encoding='utf-8') as json_file:
            # noinspection PyTypeChecker
            json.dump(metadata, json_file, indent=4, ensure_ascii=False)
    except OSError as e:
        std_msg.error(f"Failed to write metadata file: {e}")
    except TypeError as e:
        std_msg.error(f"Failed to serialize metadata to JSON: {e}")

    std_msg.info(f"Metadata saved successfully to {filenames.local_metadata}")


def upload_to_spaces(std_msg: StdMsg, args: ap.Namespace, secrets: Secrets, filenames: Filenames):
    if not all([args.bucket, args.upload_image_name,
                args.upload_metadata_name, secrets.spaces_key,
                filenames.local_image, filenames.local_metadata]):
        std_msg.error("Missing required parameters for upload_to_spaces().")
        return

    config = bc.Config(retries={'max_attempts': 3, 'mode': 'adaptive'})

    try:
        client = b3.client('s3',
                           region_name=args.region,
                           endpoint_url=filenames.spaces_endpoint_url,
                           aws_access_key_id=secrets.spaces_key,
                           aws_secret_access_key=secrets.spaces_access_key,
                           config=config)

        try:
            client.upload_file(filenames.local_image, args.bucket, args.upload_image_name,
                               ExtraArgs={"ContentType": "image/jpeg",
                                          "CacheControl": "max-age=60",
                                          "ACL": "public-read"})

        except be.ClientError as e:
            std_msg.error(f"Error occurred uploading image: ({e})")

        std_msg.info(f"Image uploaded successfully: {args.upload_image_name}")

        try:
            client.upload_file(filenames.local_metadata, args.bucket, args.upload_metadata_name,
                               ExtraArgs={"ContentType": "application/json",
                                          "CacheControl": "max-age=60",
                                          "ACL": "public-read"})

        except be.ClientError as e:
            std_msg.error(f"Exception occurred uploading metadata: ({e})")

        std_msg.info(f"Metadata uploaded successfully: {args.upload_metadata_name}")

        client.close()

    except Exception as e:
        std_msg.error(f"Error initializing S3 client: {e}")

    std_msg(f"Success! Image uploaded: {filenames.local_image}")


def main():
    std_msg = StdMsg()
    args = parse_args(std_msg)
    std_msg.set_verbose(args.verbose)
    secrets = Secrets(std_msg, args)
    capture = Capture(std_msg, args, secrets)
    filenames = Filenames(std_msg, args, capture)
    save_image(std_msg, filenames, capture)
    save_metadata(std_msg, args, filenames, capture)
    upload_to_spaces(std_msg, args, secrets, filenames)


main()
