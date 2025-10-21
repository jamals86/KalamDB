# Test script to query KalamDB
$body = @{
    sql = "SELECT * FROM jamal.user.files"
} | ConvertTo-Json

$headers = @{
    "Content-Type" = "application/json"
    "X-USER-ID" = "jamal"
}

try {
    $response = Invoke-RestMethod -Uri "http://localhost:8080/api/sql" -Method Post -Headers $headers -Body $body
    $response | ConvertTo-Json -Depth 10
} catch {
    Write-Host "Error: $_"
    Write-Host "Response: $($_.ErrorDetails.Message)"
}
