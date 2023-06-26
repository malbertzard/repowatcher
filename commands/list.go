package commands

import (
	"os"
	"repo-watch/models"
	"repo-watch/receiver"

	"github.com/olekukonko/tablewriter"
)

func ListRepositories(config *models.Config, receiver *receiver.Receiver) {
	table := tablewriter.NewWriter(os.Stdout)
	table.SetHeader([]string{"Nickname", "URL", "Folder Name"})

	for _, repo := range config.Repositories {
		row := []string{repo.Nickname, repo.URL, repo.FolderName}
		table.Append(row)
	}

	table.Render() // Output table
}
