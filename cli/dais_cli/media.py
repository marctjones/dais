"""Media upload utilities for R2 storage."""

import mimetypes
import os
import uuid
from datetime import datetime
from pathlib import Path
import subprocess
import json
from typing import Optional, List, Dict

from rich.console import Console

console = Console()

# Supported media types
SUPPORTED_IMAGE_TYPES = {
    'image/jpeg': ['.jpg', '.jpeg'],
    'image/png': ['.png'],
    'image/gif': ['.gif'],
    'image/webp': ['.webp'],
}

SUPPORTED_VIDEO_TYPES = {
    'video/mp4': ['.mp4'],
    'video/webm': ['.webm'],
}

ALL_SUPPORTED_TYPES = {**SUPPORTED_IMAGE_TYPES, **SUPPORTED_VIDEO_TYPES}

# Size limits (in bytes)
MAX_IMAGE_SIZE = 10 * 1024 * 1024  # 10 MB
MAX_VIDEO_SIZE = 40 * 1024 * 1024  # 40 MB


def get_media_type(file_path: str) -> Optional[str]:
    """
    Detect MIME type of a file.

    Args:
        file_path: Path to the file

    Returns:
        MIME type string or None if unsupported
    """
    mime_type, _ = mimetypes.guess_type(file_path)

    if mime_type in ALL_SUPPORTED_TYPES:
        return mime_type

    return None


def validate_media(file_path: str) -> tuple[bool, str]:
    """
    Validate media file for type and size.

    Args:
        file_path: Path to the file

    Returns:
        Tuple of (is_valid, error_message)
    """
    path = Path(file_path)

    # Check file exists
    if not path.exists():
        return False, f"File not found: {file_path}"

    # Check file size
    file_size = path.stat().st_size
    if file_size == 0:
        return False, "File is empty"

    # Detect MIME type
    mime_type = get_media_type(file_path)
    if not mime_type:
        return False, f"Unsupported file type. Supported: {', '.join(ALL_SUPPORTED_TYPES.keys())}"

    # Check size limits
    if mime_type.startswith('image/'):
        if file_size > MAX_IMAGE_SIZE:
            return False, f"Image too large ({file_size / 1024 / 1024:.1f}MB). Max: {MAX_IMAGE_SIZE / 1024 / 1024}MB"
    elif mime_type.startswith('video/'):
        if file_size > MAX_VIDEO_SIZE:
            return False, f"Video too large ({file_size / 1024 / 1024:.1f}MB). Max: {MAX_VIDEO_SIZE / 1024 / 1024}MB"

    return True, ""


def generate_filename(original_path: str) -> str:
    """
    Generate unique filename for R2 storage.

    Format: YYYYMMDDHHMMSS-{uuid}.{ext}
    Example: 20260309120000-a1b2c3d4.jpg

    Args:
        original_path: Original file path

    Returns:
        Generated filename
    """
    ext = Path(original_path).suffix.lower()
    timestamp = datetime.utcnow().strftime('%Y%m%d%H%M%S')
    short_uuid = str(uuid.uuid4())[:8]
    return f"{timestamp}-{short_uuid}{ext}"


def generate_media_url(filename: str, domain: str = "social.dais.social") -> str:
    """
    Generate public URL for media file.

    Args:
        filename: Name of file in R2
        domain: Base domain (default: social.dais.social)

    Returns:
        Public URL (e.g., https://social.dais.social/media/filename.jpg)
    """
    return f"https://{domain}/media/{filename}"


def upload_to_r2(file_path: str, bucket: str = "dais-media", remote: bool = False) -> Optional[str]:
    """
    Upload file to R2 bucket using wrangler.

    Args:
        file_path: Path to file to upload
        bucket: R2 bucket name
        remote: Use remote bucket (production)

    Returns:
        Filename in R2, or None if failed
    """
    # Validate file first
    is_valid, error_msg = validate_media(file_path)
    if not is_valid:
        console.print(f"[red]✗ {error_msg}[/red]")
        return None

    # Generate unique filename
    filename = generate_filename(file_path)

    console.print(f"[dim]Uploading {Path(file_path).name} to R2...[/dim]")

    # Upload using wrangler r2 object put
    cmd = ["wrangler", "r2", "object", "put", f"{bucket}/{filename}", "--file", file_path]
    if remote:
        cmd.append("--remote")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        console.print(f"[green]✓[/green] Uploaded as {filename}")
        return filename
    except subprocess.CalledProcessError as e:
        console.print(f"[red]✗ Upload failed: {e.stderr}[/red]")
        return None


def build_attachment_json(
    filenames: List[str],
    domain: str = "social.dais.social",
    alt_texts: Optional[List[str]] = None
) -> str:
    """
    Build ActivityPub attachment JSON array.

    Args:
        filenames: List of filenames in R2
        domain: Custom domain for R2 bucket
        alt_texts: Optional alt text for each attachment

    Returns:
        JSON string for database storage
    """
    attachments = []

    for i, filename in enumerate(filenames):
        # Detect media type
        mime_type = get_media_type(filename)
        if not mime_type:
            continue

        # Determine attachment type
        if mime_type.startswith('image/'):
            attachment_type = "Image"
        elif mime_type.startswith('video/'):
            attachment_type = "Video"
        else:
            attachment_type = "Document"

        # Build attachment object
        attachment = {
            "type": attachment_type,
            "mediaType": mime_type,
            "url": generate_media_url(filename, domain)
        }

        # Add alt text if provided
        if alt_texts and i < len(alt_texts):
            attachment["name"] = alt_texts[i]

        attachments.append(attachment)

    return json.dumps(attachments)


def build_attachment_dict(
    filenames: List[str],
    domain: str = "social.dais.social",
    alt_texts: Optional[List[str]] = None
) -> List[Dict]:
    """
    Build attachment list for ActivityPub Note object.

    Args:
        filenames: List of filenames in R2
        domain: Custom domain for R2 bucket
        alt_texts: Optional alt text for each attachment

    Returns:
        List of attachment dictionaries
    """
    return json.loads(build_attachment_json(filenames, domain, alt_texts))
