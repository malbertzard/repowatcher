package commands

import (
	"bufio"
	"fmt"
	"log"
	"os"
	"repo-watch/helpers"
	"repo-watch/models"
	"repo-watch/receiver"
	"strings"
)

func AddRepository(config *models.Config, configFile string, receiver receiver.Receiver) {
	if err := helpers.LoadConfig(configFile, config); err != nil {
		log.Fatal(err)
	}

	reader := bufio.NewReader(os.Stdin)

	nickname, err := readInput("Nickname: ", reader)
	if err != nil {
		log.Fatal(err)
	}

	folderName, err := readInput("Folder Name: ", reader)
	if err != nil {
		log.Fatal(err)
	}

	url, err := readInput("URL: ", reader)
	if err != nil {
		log.Fatal(err)
	}

	sparse, err := readBoolInput("Enable Sparse Checkout? (y/n): ", reader)
	if err != nil {
		log.Fatal(err)
	}

	repository := models.Repository{
		Nickname:   nickname,
		FolderName: folderName,
		URL:        url,
		Sparse:     sparse,
	}

	config.Repositories = append(config.Repositories, repository)

	if err := helpers.SaveConfig(configFile, *config); err != nil {
		log.Fatal(err)
	}

	log.Printf("Repository added: %s", repository.Nickname)
}

func readInput(prompt string, reader *bufio.Reader) (string, error) {
	fmt.Print(prompt)
	input, err := reader.ReadString('\n')
	if err != nil {
		return "", err
	}

	// Trim newline character
	input = strings.TrimSpace(input)

	return input, nil
}

func readBoolInput(prompt string, reader *bufio.Reader) (bool, error) {
	for {
		input, err := readInput(prompt, reader)
		if err != nil {
			return false, err
		}

		input = strings.ToLower(input)

		if input == "y" || input == "yes" {
			return true, nil
		} else if input == "n" || input == "no" {
			return false, nil
		} else {
			fmt.Println("Invalid input. Please enter 'y' or 'n'.")
		}
	}
}
