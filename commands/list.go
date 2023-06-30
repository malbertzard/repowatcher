package commands

import (
	"os"
	"repo-watch/models"
	"repo-watch/receiver"

	"github.com/olekukonko/tablewriter"
)

func ListRepositories(config *models.Config, receiver *receiver.Receiver) {
	table := createTable()
	populateTable(table, config.Repositories)
	renderTable(table)
}

func createTable() *tablewriter.Table {
	table := tablewriter.NewWriter(os.Stdout)
	table.SetHeader([]string{"Nickname", "URL", "Folder Name"})
	return table
}

func populateTable(table *tablewriter.Table, repositories []models.Repository) {
	for _, repo := range repositories {
		row := []string{repo.Nickname, repo.URL, repo.FolderName}
		table.Append(row)
	}
}

func renderTable(table *tablewriter.Table) {
	table.Render()
}

