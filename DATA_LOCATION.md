# Allowance Tracker Data Location

## Default Data Location

Your Allowance Tracker data is now stored in a dedicated folder to keep it organized and enable easy backups and synchronization.

**Default Location:** `~/Documents/Allowance Tracker`

On macOS, this resolves to:
```
/Users/[your-username]/Documents/Allowance Tracker
```

## Data Structure

Inside the "Allowance Tracker" folder, you'll find:

```
Allowance Tracker/
├── global_config.yaml          # Global app configuration
├── [Child Name 1]/
│   ├── child.yaml             # Child profile information
│   ├── allowance_config.yaml  # Allowance settings for this child
│   ├── transactions.csv       # Transaction history
│   └── parental_control_attempts.csv  # Login attempts log
├── [Child Name 2]/
│   ├── child.yaml
│   ├── allowance_config.yaml
│   ├── transactions.csv
│   └── parental_control_attempts.csv
└── ...
```

## Syncing with iCloud Drive (macOS)

To automatically sync your Allowance Tracker data across all your Apple devices:

### Option 1: Enable Desktop & Documents in iCloud (Recommended)
1. Open **System Settings** (or **System Preferences** on older macOS)
2. Click **Apple ID** 
3. Click **iCloud**
4. Click **iCloud Drive**
5. Make sure **Desktop & Documents Folders** is turned ON

This will automatically sync your entire Documents folder (including Allowance Tracker) to iCloud Drive.

### Option 2: Manually Move to iCloud Drive
1. Open **Finder**
2. Navigate to `~/Documents/Allowance Tracker`
3. Drag the entire "Allowance Tracker" folder to **iCloud Drive** in the sidebar
4. Wait for the upload to complete



## Manual Backup

You can easily backup your data by copying the entire "Allowance Tracker" folder:

```bash
# Create a backup
cp -r "~/Documents/Allowance Tracker" "~/Desktop/Allowance Tracker Backup $(date +%Y-%m-%d)"
```

## Accessing Data on Other Devices

### iPhone/iPad (with iCloud Drive enabled)
- Open the **Files** app
- Navigate to **iCloud Drive** > **Documents** > **Allowance Tracker**

### Other Computers (with iCloud Drive enabled)
- The data will automatically sync to `~/Documents/Allowance Tracker` on all your Macs
- On Windows with iCloud for Windows: `%USERPROFILE%/iCloudDrive/Documents/Allowance Tracker`

## Privacy and Security

- All data remains under your control in your own Documents folder or iCloud account
- No data is sent to third-party servers
- iCloud encryption protects your data in transit and storage
- You can disable iCloud sync at any time without losing local data

## Troubleshooting

**Data not syncing?**
- Check that you have sufficient iCloud storage space
- Ensure you're signed into the same Apple ID on all devices
- Check your internet connection
- Large transaction histories may take time to sync initially

**Can't find the data folder?**
- Look in Finder at **Documents** > **Allowance Tracker**
- Check the app logs for the exact path being used
- The path is logged when the app starts up

**Migration not working?**
- Check that you have write permissions to the Documents folder
- Ensure there's enough disk space for the copy operation
- The app logs will show detailed migration progress 