function Package-Source {
	param (
		[Parameter(Mandatory = $true, Position = 0)]
		[String[]]$Name,
		[switch]$Build
	)
	$Name | ForEach-Object	{
		$source = $_
		if ($Build) {
			Write-Output "building $source"
			cargo +nightly build --release
		}
		Write-Output "packaging $source"
		New-Item -ItemType Directory -Path target/wasm32-unknown-unknown/release/Payload -Force | Out-Null
		Copy-Item res/* target/wasm32-unknown-unknown/release/Payload -ErrorAction SilentlyContinue
		Set-Location target/wasm32-unknown-unknown/release
		Copy-Item *.wasm Payload/main.wasm
		# PowerShell supports only .zip directly; create .zip then rename to .aix
		# Zip the contents of Payload so files are at archive root
		Compress-Archive -Force -DestinationPath "../../../package.zip" -Path Payload/*
		if (Test-Path "../../../package.aix") { Remove-Item "../../../package.aix" -Force }
		Rename-Item -Path "../../../package.zip" -NewName "package.aix"
		Remove-Item -Recurse -Force Payload/
		Set-Location ../../..
	}
}
Package-Source siren_scans -Build
