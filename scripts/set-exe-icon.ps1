param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,

    [Parameter(Mandatory = $true)]
    [string]$IconPath
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class NativeMethods
{
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern IntPtr BeginUpdateResource(string pFileName, [MarshalAs(UnmanagedType.Bool)] bool bDeleteExistingResources);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool UpdateResource(
        IntPtr hUpdate,
        IntPtr lpType,
        IntPtr lpName,
        ushort wLanguage,
        byte[] lpData,
        uint cbData);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool EndUpdateResource(IntPtr hUpdate, [MarshalAs(UnmanagedType.Bool)] bool fDiscard);
}
'@

function Write-UInt16LE {
    param(
        [System.IO.BinaryWriter]$Writer,
        [UInt16]$Value
    )

    $Writer.Write([BitConverter]::GetBytes($Value))
}

function Write-UInt32LE {
    param(
        [System.IO.BinaryWriter]$Writer,
        [UInt32]$Value
    )

    $Writer.Write([BitConverter]::GetBytes($Value))
}

function Get-IconPayload {
    param([string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 6) {
        throw "ICO file is too small: $Path"
    }

    $reserved = [BitConverter]::ToUInt16($bytes, 0)
    $type = [BitConverter]::ToUInt16($bytes, 2)
    $count = [BitConverter]::ToUInt16($bytes, 4)

    if ($reserved -ne 0 -or $type -ne 1 -or $count -lt 1) {
        throw "Invalid ICO header in $Path"
    }

    $entries = @()
    for ($index = 0; $index -lt $count; $index++) {
        $offset = 6 + ($index * 16)
        if ($offset + 16 -gt $bytes.Length) {
            throw "ICO directory entry $index is truncated in $Path"
        }

        $width = $bytes[$offset]
        $height = $bytes[$offset + 1]
        $colorCount = $bytes[$offset + 2]
        $reservedByte = $bytes[$offset + 3]
        $planes = [BitConverter]::ToUInt16($bytes, $offset + 4)
        $bitCount = [BitConverter]::ToUInt16($bytes, $offset + 6)
        $bytesInRes = [BitConverter]::ToUInt32($bytes, $offset + 8)
        $imageOffset = [BitConverter]::ToUInt32($bytes, $offset + 12)

        if (($imageOffset + $bytesInRes) -gt $bytes.Length) {
            throw "ICO image $index points outside the file in $Path"
        }

        $imageData = New-Object byte[] $bytesInRes
        [Array]::Copy($bytes, [int]$imageOffset, $imageData, 0, [int]$bytesInRes)

        $entries += [PSCustomObject]@{
            Width      = $width
            Height     = $height
            ColorCount = $colorCount
            Reserved   = $reservedByte
            Planes     = $planes
            BitCount   = $bitCount
            BytesInRes = $bytesInRes
            ImageData  = $imageData
        }
    }

    return $entries
}

function Set-ExeIcon {
    param(
        [string]$ExePathValue,
        [string]$IconPathValue
    )

    if (-not (Test-Path -LiteralPath $ExePathValue)) {
        throw "Executable not found: $ExePathValue"
    }
    if (-not (Test-Path -LiteralPath $IconPathValue)) {
        throw "Icon not found: $IconPathValue"
    }

    $entries = Get-IconPayload -Path $IconPathValue
    $groupName = [IntPtr]1
    $language = [UInt16]0
    $iconBaseId = 101
    $RT_ICON = [IntPtr]3
    $RT_GROUP_ICON = [IntPtr]14

    $handle = [NativeMethods]::BeginUpdateResource($ExePathValue, $false)
    if ($handle -eq [IntPtr]::Zero) {
        throw "BeginUpdateResource failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }

    try {
        $memory = New-Object System.IO.MemoryStream
        $writer = New-Object System.IO.BinaryWriter($memory)
        Write-UInt16LE -Writer $writer -Value 0
        Write-UInt16LE -Writer $writer -Value 1
        Write-UInt16LE -Writer $writer -Value ([UInt16]$entries.Count)

        for ($index = 0; $index -lt $entries.Count; $index++) {
            $entry = $entries[$index]
            $resourceId = [UInt16]($iconBaseId + $index)
            $resourceName = [IntPtr]$resourceId

            if (-not [NativeMethods]::UpdateResource($handle, $RT_ICON, $resourceName, $language, $entry.ImageData, [UInt32]$entry.BytesInRes)) {
                throw ("UpdateResource RT_ICON failed for id {0}: {1}" -f $resourceId, [Runtime.InteropServices.Marshal]::GetLastWin32Error())
            }

            $writer.Write([byte]$entry.Width)
            $writer.Write([byte]$entry.Height)
            $writer.Write([byte]$entry.ColorCount)
            $writer.Write([byte]$entry.Reserved)
            Write-UInt16LE -Writer $writer -Value ([UInt16]$entry.Planes)
            Write-UInt16LE -Writer $writer -Value ([UInt16]$entry.BitCount)
            Write-UInt32LE -Writer $writer -Value ([UInt32]$entry.BytesInRes)
            Write-UInt16LE -Writer $writer -Value $resourceId
        }

        $groupData = $memory.ToArray()
        if (-not [NativeMethods]::UpdateResource($handle, $RT_GROUP_ICON, $groupName, $language, $groupData, [UInt32]$groupData.Length)) {
            throw ("UpdateResource RT_GROUP_ICON failed: {0}" -f [Runtime.InteropServices.Marshal]::GetLastWin32Error())
        }
    }
    catch {
        [void][NativeMethods]::EndUpdateResource($handle, $true)
        throw
    }

    if (-not [NativeMethods]::EndUpdateResource($handle, $false)) {
        throw "EndUpdateResource failed: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
}

Set-ExeIcon -ExePathValue $ExePath -IconPathValue $IconPath
