resource "google_project_iam_member" "dev_jobuser" {
  for_each = local.lana_dev_users
  project  = local.project
  role     = "roles/bigquery.jobUser"
  member   = "user:${each.value}"
}

resource "google_project_iam_member" "read_session_user" {
  for_each = local.lana_dev_users
  project  = local.project
  role     = "roles/bigquery.readSessionUser"
  member   = "user:${each.value}"
}
