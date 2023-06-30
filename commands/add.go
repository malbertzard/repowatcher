package commands

import (
	"fmt"
	"log"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
	"strings"
)

func AddRepository(config *models.Config, configFile string, receiver receiver.Receiver) {
	if err := helpers.LoadConfig(configFile, config); err != nil {
		log.Fatalf("Failed to load config file: %v", err)
	}

	nickname := readInput("Nickname: ")
	folderName := readInput("Folder Name: ")
	url := readInput("URL: ")
	sparse := readBoolInput("Enable Sparse Checkout? (y/n): ")

	repository := models.Repository{
		Nickname:   nickname,
		FolderName: folderName,
		URL:        url,
		Sparse:     sparse,
	}

	config.Repositories = append(config.Repositories, repository)

	if err := helpers.SaveConfig(configFile, *config); err != nil {
		log.Fatalf("Failed to save config file: %v", err)
	}

	log.Printf("Repository added: %s", repository.Nickname)
}

func readInput(prompt string) string {
	var input string
	fmt.Print(prompt)
	fmt.Scanln(&input)
	return strings.TrimSpace(input)
}

func readBoolInput(prompt string) bool {
	for {
		input := readInput(prompt)

		switch strings.ToLower(input) {
		case "y", "yes":
			return true
		case "n", "no":
			return false
		default:
			fmt.Println("Invalid input. Please enter 'y' or 'n'.")
		}
	}
}

